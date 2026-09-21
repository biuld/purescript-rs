use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use psrs_driver::compile_source;

#[test]
fn executes_a_compiled_component_with_the_required_wasmtime_baseline() {
    let version = Command::new("wasmtime").arg("--version").output();
    let required = std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1");
    let Ok(version) = version else {
        if required {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
        }
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    if !version.status.success() {
        if required {
            panic!("PSRS_REQUIRE_WASMTIME=1 but `wasmtime --version` failed: {version:?}");
        }
        eprintln!("skipping: wasmtime is unusable");
        return;
    }

    let source = "module Main where\nmain = 42\n";
    let artifact = compile_source("Main.purs", source).expect("compiling the execution gate");
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "psrs-required-wasmtime-{}-{id}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, artifact.wasm).expect("writing the execution-gate artifact");
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("running the execution-gate artifact");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        output.status.code(),
        Some(42),
        "wasmtime could not execute the compiled component: {output:?}"
    );
}
