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
