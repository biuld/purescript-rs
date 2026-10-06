//! The library's `Show` class.
//!
//! These tests execute under Wasmtime. The strings are the official `Show`
//! spellings: `true`/`false`, decimal `Int`, `unit`, quoted `Char` and
//! `String` (with the official escapes), arrays without spaces, and `Number`
//! with a trailing `.0` on an integer token.

use super::*;

#[test]
fn show_renders_the_instances_the_corpus_prints() {
    let source = r#"
module Main where

import Prelude
import Effect.Console (log)

checks :: Effect Unit
checks = do
  log (show true)
  log (show false)
  log (show 0)
  log (show 42)
  log (show (0 - 7))
  log (show ((0 - 2147483647) - 1))
  log (show unit)
  log (show "hi")
  log (show "a\"b")
  log (show "line\n")
  log (show 'a')
  log (show '\n')
  log (show '\'')
  log (show [1, 2, 3])
  log (show [1.0, 2.0])
  log (show 0.0)
  log (show 1.0)
  log (show (numberNeg 2.0))
  log (show 1.5)
  log (show 0.5)
  log (show 0.25)
  log (show 10000000000.0)
  log (show 1.0e21)
  log (show 1.0e-5)
  log (show "é")
  log (show "cafés")
  pure unit

main = let ignored = runEffect checks in 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected = concat!(
        "true\n",
        "false\n",
        "0\n",
        "42\n",
        "-7\n",
        "-2147483648\n",
        "unit\n",
        "\"hi\"\n",
        "\"a\\\"b\"\n",
        "\"line\\n\"\n",
        "'a'\n",
        "'\\n'\n",
        "'\\''\n",
        "[1,2,3]\n",
        "[1.0,2.0]\n",
        "0.0\n",
        "1.0\n",
        "-2.0\n",
        "1.5\n",
        "0.5\n",
        "0.25\n",
        "10000000000.0\n",
        "1e+21\n",
        "0.00001\n",
        "\"é\"\n",
        "\"cafés\"\n",
    );
    assert_eq!(
        stdout.as_ref(),
        expected,
        "stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn formatter_plan_records_the_pinned_artifact_digest() {
    let source = r#"
module Main where

import Prelude
import Effect.Console (log)

main = let ignored = log (show 1.0e21) in 0
"#;
    let report = compile_program_sources_with_prelude_diagnosis(&[("Main.purs", source)], false);
    assert!(
        report.artifact.is_some(),
        "show should compile: {:?}",
        report.diagnostics
    );
    let trace = report.backend_trace.expect("backend pass trace");
    let plan = trace
        .executions
        .iter()
        .find(|execution| execution.pass_key == "backend.target.plan")
        .expect("the checked plan is an observed execution");
    let parameter = |key: &str| {
        plan.parameters
            .iter()
            .find(|parameter| parameter.key == key)
            .map(|parameter| parameter.value.as_str())
    };
    assert_eq!(parameter("artifacts"), Some("1"));
    let digests = parameter("artifact_digests").expect("artifact digests are recorded");
    assert!(digests.contains("psrs:runtime-number-format"), "{digests}");
    assert!(
        parameter("selected_providers")
            .expect("selected providers are recorded")
            .contains("NumberToString"),
    );
}

#[test]
fn show_covers_number_and_aggregate_boundaries() {
    let source = r#"
module Main where

import Prelude
import Effect.Console (log)

checks :: Effect Unit
checks = do
  log (show (0.0 / 0.0))
  log (show (1.0 / 0.0))
  log (show ((0.0 - 1.0) / 0.0))
  log (show (numberNeg 0.0))
  log (show 1.0e-6)
  log (show 1.0e-7)
  log (show 1.0e20)
  log (show 5.0e-324)
  log (show ([] :: Array Int))
  log (show [[1, 2], [3]])
  pure unit

main = let ignored = runEffect checks in 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected = concat!(
        "NaN\n",
        "Infinity\n",
        "-Infinity\n",
        "0.0\n",
        "0.000001\n",
        "1e-7\n",
        "100000000000000000000.0\n",
        "5e-324\n",
        "[]\n",
        "[[1,2],[3]]\n",
    );
    assert_eq!(
        stdout.as_ref(),
        expected,
        "stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn formats_many_numbers_without_exhausting_the_runtime_stack() {
    // The formatter is nonrecursive; a large array of numbers calls its raw
    // export once per element through the same private stack region.
    let mut elements = String::new();
    for index in 0..64 {
        if index > 0 {
            elements.push(',');
        }
        elements.push_str(&format!("{index}.5"));
    }
    let source = format!(
        "module Main where\n\nimport Prelude\nimport Effect.Console (log)\n\nmain = let ignored = runEffect (log (show [{elements}])) in 0\n"
    );
    let Some(output) = run_with_wasmtime(&source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("[0.5,1.5,") && stdout.ends_with("63.5]\n"),
        "stdout should hold every formatted element: {stdout}"
    );
}

#[test]
fn a_type_without_show_is_rejected() {
    let source = r#"
module Main where

import Prelude

data Box = Box

main :: String
main = show Box
"#;
    let errors = check_program_types_lenient_with_prelude(&[("Main.purs", source)])
        .expect_err("Box is not a Show");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
        "expected NoInstanceFound, got {errors:?}"
    );
}
