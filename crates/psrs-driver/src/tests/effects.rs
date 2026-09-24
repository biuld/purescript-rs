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

fn run_effect_program(source: &str) -> Option<std::process::Output> {
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        return None;
    }
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let artifact = compile_source("Main.purs", source).unwrap();
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("psrs-effect-{}-{id}.wasm", std::process::id()));
    std::fs::write(&path, &artifact.wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(path);
    Some(output)
}
