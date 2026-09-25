//! Value-sensitive execution and structural evidence for the CC IR topic.
//!
//! Every execution case runs the normal P8-to-Wasm pipeline and is executed
//! with Wasmtime when it is available. Structural cases lower Typed Core and
//! inspect the verified CC module before P9.

use super::*;
use psrs_backend::cc::{AssignmentKind, Function};
use std::collections::HashSet;

fn required_wasmtime() -> bool {
    std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1")
}

fn execute_wasm(name: &str, wasm: &[u8]) -> Result<Option<std::process::Output>, String> {
    let available = std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available {
        if required_wasmtime() {
            return Err(format!("wasmtime is unavailable for `{name}`"));
        }
        eprintln!("skipping `{name}`: wasmtime is not installed");
        return Ok(None);
    }
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("psrs-cc-ir-{}-{id}.wasm", std::process::id()));
    std::fs::write(&path, wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Ok(Some(output))
}

fn expect_source_exit(name: &str, source: &str, expected: i32) {
    let artifact = match compile_source("Main.purs", source) {
        Ok(artifact) => artifact,
        Err(errors) => panic!("`{name}` failed to compile: {errors:?}"),
    };
    if let Some(output) = execute_wasm(name, &artifact.wasm).expect("executing the component") {
        assert_eq!(
            output.status.code(),
            Some(expected),
            "`{name}` produced the wrong exit code; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn expect_prelude_program_exit(name: &str, sources: &[(&str, &str)], expected: i32) {
    let artifact = match compile_program_sources_with_prelude(sources) {
        Ok(artifact) => artifact,
        Err(errors) => {
            let messages = errors
                .into_iter()
                .map(|error| error.diagnostic.message)
                .collect::<Vec<_>>();
            panic!("`{name}` failed to compile: {messages:?}");
        }
    };
    if let Some(output) = execute_wasm(name, &artifact.wasm).expect("executing the component") {
        assert_eq!(output.status.code(), Some(expected), "`{name}` exit code");
    }
}

fn cc_functions(source: &str) -> Vec<Function> {
    let core = lower_source_to_core("Main.purs", source).expect("source lowers to Core");
    psrs_backend::compile_with_stages(core)
        .expect("Core lowers through CC to Wasm")
        .cc
        .functions
}

const RECURSIVE_SUM: &str = "module Main where\nimport Prelude\ng :: Int -> Int -> Int\ng n acc = if n == 0 then acc else g (n - 1) (acc + 1)\n";

/// CC-01, CC-06: parameters lead the value declarations, symbols are unique,
/// and the one lifted closure captures its free locals once, in order.
#[test]
fn cc_declarations_are_well_formed_and_capture_once() {
    let source = "module Main where\nimport Prelude\napply :: (Int -> Int) -> Int -> Int\napply f x = f x\nmain = let a = 1 in let b = 2 in apply (\\x -> a + b + x + a) 0\n";
    let functions = cc_functions(source);
    let symbols = functions
        .iter()
        .map(|function| function.symbol)
        .collect::<HashSet<_>>();
    assert_eq!(symbols.len(), functions.len(), "CC symbols must be unique");
    for function in &functions {
        let prefix = function
            .values
            .iter()
            .take(function.parameters.len())
            .map(|value| value.id)
            .collect::<Vec<_>>();
        assert_eq!(
            prefix, function.parameters,
            "parameters must be the first value declarations"
        );
        assert!(
            function
                .assignments
                .iter()
                .all(|assignment| assignment.span.start <= assignment.span.end),
            "every assignment keeps a well-formed source span"
        );
    }
    let capturing = functions
        .iter()
        .filter(|function| {
            function.assignments.iter().any(|assignment| {
                matches!(assignment.kind, AssignmentKind::ClosureGetCapture { .. })
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        capturing.len(),
        1,
        "exactly one lifted closure captures locals"
    );
    let indices = capturing[0]
        .assignments
        .iter()
        .filter_map(|assignment| match assignment.kind {
            AssignmentKind::ClosureGetCapture { index, .. } => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        indices,
        vec![0, 1],
        "captures are ordered and deduplicated on first use"
    );
}

/// CC-02: the callee is evaluated before its arguments and arguments are
/// evaluated left to right, observed through WASI output.
#[test]
fn effects_observe_callee_and_argument_order() {
    let name = "effects_observe_callee_and_argument_order";
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nfirst :: Unit -> Unit -> Int\nfirst a b = 0\nmain = first (runEffect (log \"a\")) (runEffect (log \"b\"))\n";
    let artifact = compile_source("Main.purs", source).expect("ordered effects compile");
    if let Some(output) = execute_wasm(name, &artifact.wasm).expect("executing ordered effects") {
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"a\nb\n", "arguments evaluate left to right");
    }
}

/// CC-07: a partial application captures its supplied argument once and the
/// resulting function value executes.
#[test]
fn partial_application_value_executes() {
    let source = format!(
        "{RECURSIVE_SUM}use :: (Int -> Int) -> Int\nuse h = h 0\nmain :: Int\nmain = use (g 42)\n"
    );
    expect_source_exit("partial_application_value_executes", &source, 42);
}

/// CC-07: an over-applied global function value is evaluated and called
/// indirectly instead of failing arity checking.
#[test]
fn over_application_of_a_global_value_executes() {
    let source = format!(
        "{RECURSIVE_SUM}alias :: Int -> Int -> Int\nalias = g\nmain :: Int\nmain = alias 42 0\n"
    );
    expect_source_exit("over_application_of_a_global_value_executes", &source, 42);
}

/// CC-07: a declaration that binds fewer parameters than its type has arrows
/// returns a closure; its wrapper exposes the full type and executes.
#[test]
fn global_value_with_partial_lambda_prefix_executes() {
    let source = format!(
        "{RECURSIVE_SUM}f :: Int -> Int -> Int\nf x = g x\nuse :: (Int -> Int -> Int) -> Int\nuse h = h 42 0\nmain :: Int\nmain = use f\n"
    );
    expect_source_exit(
        "global_value_with_partial_lambda_prefix_executes",
        &source,
        42,
    );
}

/// CC-04, CC-07: partial applications in linked source modules receive
/// distinct stable symbols and both execute.
#[test]
fn partial_applications_in_linked_modules_get_distinct_symbols() {
    let one = "module One where\nimport Prelude\nsumTo :: Int -> Int -> Int\nsumTo n acc = if n == 0 then acc else sumTo (n - 1) acc\npartial :: Int -> Int\npartial = sumTo 42\n";
    let two = "module Two where\nimport Prelude\nsumTo :: Int -> Int -> Int\nsumTo n acc = if n == 0 then acc else sumTo (n - 1) acc\npartial :: Int -> Int\npartial = sumTo 42\n";
    let main = "module Main where\nimport Prelude\nimport One\nimport Two\nuse2 :: (Int -> Int) -> (Int -> Int) -> Int\nuse2 a b = a (b 42)\nmain :: Int\nmain = use2 One.partial Two.partial\n";
    expect_prelude_program_exit(
        "partial_applications_in_linked_modules_get_distinct_symbols",
        &[("One.purs", one), ("Two.purs", two), ("Main.purs", main)],
        42,
    );
}

/// CC-06, CC-07: a multi-argument nested lambda is one lifted closure, and a
/// nested lambda that returns a closure is eta-expanded to its full arity.
#[test]
fn nested_lambda_arities_execute() {
    expect_source_exit(
        "nested_lambda_arities_execute",
        "module Main where\nimport Prelude\napply2 :: (Int -> Int -> Int) -> Int\napply2 f = f 40 2\nmain :: Int\nmain = apply2 (\\x -> \\y -> x + y)\n",
        42,
    );
    expect_source_exit(
        "nested_lambda_returning_closure_executes",
        "module Main where\nimport Prelude\ng :: Int -> Int -> Int\ng x y = x + y\napply2 :: (Int -> Int -> Int) -> Int\napply2 h = h 40 2\nmain :: Int\nmain = let f = \\x -> g x in apply2 f\n",
        42,
    );
}

/// CC-08: a polymorphic function value adapted across an erased boundary is
/// captured once and executes.
#[test]
fn erased_function_adapter_executes() {
    expect_source_exit(
        "erased_function_adapter_executes",
        "module Main where\nidentity :: forall a. a -> a\nidentity value = value\nmain = let f = identity (\\value -> value) in f 42\n",
        42,
    );
}
