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
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("psrs-loader-{}-{id}.wasm", std::process::id()));
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

#[test]
fn loads_the_standard_library_from_disk_in_trusted_order() {
    let modules = crate::prelude::sources().expect("the standard library should load from disk");
    let names = modules
        .iter()
        .map(|module| module.module_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["Prelude", "WASI.Console", "WASI.Clock", "WASI.Random"]
    );
    for module in modules {
        let path = std::path::Path::new(&module.path);
        assert!(
            path.ends_with("lib/Prelude.purs")
                || path.ends_with("lib/WASI/Console.purs")
                || path.ends_with("lib/WASI/Clock.purs")
                || path.ends_with("lib/WASI/Random.purs"),
            "{}",
            module.path
        );
        let on_disk =
            std::fs::read_to_string(path).expect("the standard-library file should exist");
        assert_eq!(on_disk, module.text, "{}", module.path);
    }
    check_source(
        "Main.purs",
        "module Main where\nimport Prelude\nimport WASI.Console\nimport WASI.Clock\nmain = let stamp = runEffect now in let action = runEffect (log \"ok\") in stamp\n",
    )
    .expect("the on-disk standard library should typecheck with a user module");
}

#[test]
fn does_not_discover_a_user_module_shadowing_the_standard_library() {
    let directory = temp_directory("stdlib-shadow");
    let local_prelude = directory.join("Prelude.purs");
    let main = directory.join("Main.purs");
    std::fs::write(
        &local_prelude,
        "module Prelude where\nshadow :: Int\nshadow = 1\n",
    )
    .unwrap();
    std::fs::write(
        &main,
        "module Main where\nimport Prelude\nmain = runEffect (pure 1)\n",
    )
    .unwrap();

    let loaded = load_program_files(&[main.to_string_lossy().into_owned()])
        .expect("the loader should read the entry file");
    assert_eq!(
        loaded.len(),
        1,
        "the on-disk Prelude is not rediscovered: {loaded:?}"
    );
    assert!(
        !loaded
            .iter()
            .any(|(path, _)| path.ends_with("Prelude.purs")),
        "{loaded:?}"
    );
    let inputs = loaded
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let artifact = compile_program_sources_with_prelude(&inputs)
        .expect("the trusted Prelude should still compile");
    if let Some(code) = run_component(&artifact.wasm) {
        assert_eq!(code, 1);
    }
    let _ = std::fs::remove_dir_all(&directory);
}
