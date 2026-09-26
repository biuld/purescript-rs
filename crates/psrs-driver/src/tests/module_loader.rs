//! Filesystem module-loader discovery (WASI-09).

use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

fn temp_directory(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory =
        std::env::temp_dir().join(format!("psrs-loader-{tag}-{}-{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("creating a temp directory");
    directory
}

fn run_component(wasm: &[u8]) -> Option<i32> {
    let available = std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available {
        if std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1") {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
        }
        return None;
    }
    let path = std::env::temp_dir().join(format!("psrs-loader-{}.wasm", std::process::id()));
    std::fs::write(&path, wasm).expect("writing the component");
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("running the component");
    let _ = std::fs::remove_file(&path);
    output.status.code()
}

#[test]
fn discovers_an_imported_module_from_the_entry_directory() {
    let directory = temp_directory("discover");
    let helper = directory.join("Helper.purs");
    let main = directory.join("Main.purs");
    std::fs::write(&helper, "module Helper where\nanswer :: Int\nanswer = 42\n").unwrap();
    std::fs::write(
        &main,
        "module Main where\nimport Helper\nmain :: Int\nmain = answer\n",
    )
    .unwrap();

    let loaded = load_program_files(&[main.to_string_lossy().into_owned()])
        .expect("the loader should read the entry file");
    assert_eq!(loaded.len(), 2, "both modules should be loaded: {loaded:?}");
    assert!(
        loaded.iter().any(|(path, _)| path.ends_with("Helper.purs")),
        "{loaded:?}"
    );

    let inputs = loaded
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let artifact =
        compile_program_sources_with_prelude(&inputs).expect("the program should compile");
    if let Some(code) = run_component(&artifact.wasm) {
        assert_eq!(code, 42);
    }
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn loads_only_the_modules_that_are_imported() {
    let directory = temp_directory("unused");
    let used = directory.join("Used.purs");
    let unused = directory.join("Unused.purs");
    let main = directory.join("Main.purs");
    std::fs::write(&used, "module Used where\nanswer :: Int\nanswer = 7\n").unwrap();
    std::fs::write(&unused, "module Unused where\nanswer :: Int\nanswer = 0\n").unwrap();
    std::fs::write(&main, "module Main where\nimport Used\nmain = answer\n").unwrap();

    let loaded = load_program_files(&[main.to_string_lossy().into_owned()])
        .expect("the loader should read the entry file");
    assert_eq!(loaded.len(), 2, "only Main and Used load: {loaded:?}");
    assert!(
        !loaded.iter().any(|(path, _)| path.ends_with("Unused.purs")),
        "{loaded:?}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}
