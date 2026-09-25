use super::super::*;
use std::sync::atomic::{AtomicU32, Ordering};

#[test]
fn constructing_an_effect_does_not_execute_it() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let action = log \"not printed\" in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
}

#[test]
fn running_effects_preserves_source_order() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let first = runEffect (log \"first\") in let second = runEffect (log \"second\") in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"first\nsecond\n");
}

#[test]
fn a_stored_effect_runs_each_time_it_is_explicitly_run() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let action = log \"again\" in let first = runEffect action in let second = runEffect action in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"again\nagain\n");
}

#[test]
fn a_function_cannot_be_passed_to_run_effect_as_an_effect() {
    let source = "module Main where\nimport Prelude\nmain = runEffect (\\token -> 42)\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(
        errors.iter().any(|error| {
            error.stage == "P5 typecheck" && error.message.contains("type mismatch")
        })
    );
}

#[test]
fn run_effect_is_only_available_from_the_selected_entry() {
    let helper = (
        "Helper.purs",
        "module Helper where\nimport Prelude\nrun = runEffect (pure 42)\n",
    );
    let main = (
        "Main.purs",
        "module Main where\nimport Helper\nmain = run\n",
    );
    let errors = compile_program_sources_with_prelude(&[helper, main]).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.diagnostic.stage == "P7 entry selection"
            && error
                .diagnostic
                .message
                .contains("runEffect` binding may only be referenced")
    }));
}

#[test]
fn transitive_effect_types_keep_their_closure_representation() {
    let library_source = (
        "Library.purs",
        "module Library where\nimport Prelude\nimport WASI.Clock\naction :: Effect Int\naction = now\n",
    );
    let main_source = (
        "Main.purs",
        "module Main where\nimport Library\nforward = action\nmain = let ignored = forward in 0\n",
    );
    let mut sources = prelude::SOURCES.to_vec();
    sources.extend([library_source, main_source]);
    let typed = crate::program::typecheck_program_sources_with_trusted_prefix(
        &sources,
        prelude::SOURCES.len(),
    )
    .unwrap();
    let library = typed
        .iter()
        .find(|module| module.name == "Library")
        .unwrap();
    let main = typed.iter().find(|module| module.name == "Main").unwrap();
    let action = library
        .declarations
        .iter()
        .find(|declaration| declaration.name == "action")
        .unwrap();
    let forward = main
        .declarations
        .iter()
        .find(|declaration| declaration.name == "forward")
        .unwrap();
    assert!(matches!(
        library.types.get(action.ty.0 as usize),
        Some(psrs_thir::Type::Function { .. })
    ));
    assert_eq!(
        main.types.get(forward.ty.0 as usize),
        library.types.get(action.ty.0 as usize)
    );
    let artifact = compile_program_sources_with_prelude(&[library_source, main_source]);
    assert!(artifact.is_ok(), "{artifact:?}");
}

#[test]
fn an_untrusted_prelude_effect_remains_an_ordinary_user_type() {
    let prelude_source = (
        "Prelude.purs",
        "module Prelude where\ndata Effect a = MkEffect a\nidentity :: Effect Int\nidentity = MkEffect 42\n",
    );
    let main_source = (
        "Main.purs",
        "module Main where\nimport Prelude\nforward :: Effect Int\nforward = identity\nmain = 0\n",
    );
    let typed = crate::program::typecheck_program_sources(&[prelude_source, main_source]).unwrap();
    let main = typed.iter().find(|module| module.name == "Main").unwrap();
    let forward = main
        .declarations
        .iter()
        .find(|declaration| declaration.name == "forward")
        .unwrap();
    let psrs_thir::Type::Application(effect_constructor, _) = &main.types[forward.ty.0 as usize]
    else {
        panic!("untrusted Prelude.Effect should remain an applied user type");
    };
    assert!(matches!(
        main.types[effect_constructor.0 as usize],
        psrs_thir::Type::Constructor(psrs_thir::TypeConstructor::User(_))
    ));
    assert!(compile_program_sources(&[prelude_source, main_source]).is_ok());
}

#[test]
fn running_pure_returns_the_supplied_value_without_an_external_action() {
    let source = "module Main where\nimport Prelude\nmain = runEffect (pure 42)\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
    assert!(
        output.stdout.is_empty(),
        "pure must not perform an external action: {output:?}"
    );
}

#[test]
fn bind_runs_the_first_effect_before_the_continuation() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let action = bind (log \"first\") (\\_ -> log \"second\") in let result = runEffect action in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"first\nsecond\n");
}

#[test]
fn bind_passes_the_first_result_to_the_continuation() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let action = bind (pure 1) (\\x -> if x < 2 then log \"small\" else log \"big\") in let result = runEffect action in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"small\n");
}

#[test]
fn nested_binds_preserve_left_to_right_order() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let action = bind (log \"a\") (\\_ -> bind (log \"b\") (\\_ -> log \"c\")) in let result = runEffect action in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"a\nb\nc\n");
}

#[test]
fn bind_runs_the_returned_effect_exactly_once() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let action = bind (log \"once\") (\\_ -> log \"tail\") in let result = runEffect action in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"once\ntail\n");
}

#[test]
fn an_effect_captured_by_a_closure_is_not_run() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let action = log \"captured\" in let holder = \\n -> action in let stored = holder in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stdout.is_empty(),
        "capturing an effect must not run it: {output:?}"
    );
}

#[test]
fn passing_an_effect_to_a_function_does_not_run_it() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nconsume :: forall a. Effect a -> Int\nconsume effect = 0\nmain = let action = log \"passed\" in consume action\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stdout.is_empty(),
        "passing an effect must not run it: {output:?}"
    );
}

#[test]
fn a_polymorphic_effect_uses_ordinary_adapters() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nidentityEffect :: forall a. Effect a -> Effect a\nidentityEffect effect = effect\nmain = let action = identityEffect (log \"generic\") in let result = runEffect action in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"generic\n");
}

#[test]
fn a_polymorphic_effect_carries_string_and_number_values() {
    let string = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let action = bind (pure \"s\") (\\_ -> log \"str\") in let result = runEffect action in 0\n";
    let Some(output) = run_effect_program(string) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"str\n");

    let number = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let action = bind (pure 1.5) (\\_ -> log \"num\") in let result = runEffect action in 0\n";
    let Some(output) = run_effect_program(number) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"num\n");
}

#[test]
fn a_polymorphic_effect_carries_a_gc_aggregate() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\ndata Pair a = Pair a a\nidentityEffect :: forall a. Effect a -> Effect a\nidentityEffect effect = effect\nmain = let action = identityEffect (pure (Pair 1 2)) in let result = runEffect (bind action (\\p -> log \"poly\")) in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"poly\n");
}

#[test]
fn bind_preserves_wasi_results_across_stdout_and_stderr() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let action = bind (error \"stderr\") (\\_ -> log \"stdout\") in let result = runEffect action in 0\n";
    let Some(output) = run_effect_program(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"stdout\n");
    assert_eq!(output.stderr, b"stderr\n");
}

#[test]
fn a_linked_module_forwards_an_effect_without_running_it() {
    let producer = (
        "Producer.purs",
        "module Producer where\nimport Prelude\nimport WASI.Console\naction :: Effect Unit\naction = log \"linked\"\n",
    );
    let main = (
        "Main.purs",
        "module Main where\nimport Prelude\nimport Producer\nforward :: Effect Unit\nforward = action\nmain = let stored = forward in 0\n",
    );
    let Some(output) = run_effect_program_sources(&[producer, main]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stdout.is_empty(),
        "forwarding an effect must not run it: {output:?}"
    );
}

#[test]
fn a_linked_module_forwards_an_effect_that_runs_only_when_selected() {
    let producer = (
        "Producer.purs",
        "module Producer where\nimport Prelude\nimport WASI.Console\naction :: Effect Unit\naction = log \"linked\"\n",
    );
    let main = (
        "Main.purs",
        "module Main where\nimport Prelude\nimport Producer\nforward :: Effect Unit\nforward = action\nmain = let result = runEffect forward in 0\n",
    );
    let Some(output) = run_effect_program_sources(&[producer, main]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"linked\n");
}

fn run_effect_program(source: &str) -> Option<std::process::Output> {
    if !wasmtime_available() {
        return None;
    }
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let artifact = compile_source("Main.purs", source).unwrap();
    run_artifact(&artifact.wasm, "effect", &COUNTER)
}

/// Runs a program linked from several source modules with the embedded
/// standard library.
fn run_effect_program_sources(sources: &[(&str, &str)]) -> Option<std::process::Output> {
    if !wasmtime_available() {
        return None;
    }
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let artifact = compile_program_sources_with_prelude(sources).unwrap();
    run_artifact(&artifact.wasm, "effect-multi", &COUNTER)
}

fn wasmtime_available() -> bool {
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok()
    {
        return true;
    }
    if std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1") {
        panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
    }
    false
}

fn run_artifact(wasm: &[u8], tag: &str, counter: &AtomicU32) -> Option<std::process::Output> {
    let id = counter.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("psrs-{tag}-{}-{id}.wasm", std::process::id()));
    std::fs::write(&path, wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(path);
    Some(output)
}
