use std::process::Command;

pub fn execute(name: &str, source: &str, expected: i32) {
    run(name, source, expected, false);
}

#[allow(dead_code)]
pub fn execute_with_gc(name: &str, source: &str, expected: i32) {
    run(name, source, expected, true);
}

fn run(name: &str, source: &str, expected: i32, force_gc: bool) {
    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert_ne!(
            std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(),
            Ok("1"),
            "control execution requires wasmtime"
        );
        eprintln!("skipping control execution: wasmtime is unavailable");
        return;
    }
    let artifact = psrs_driver::compile_source("Main.purs", source)
        .unwrap_or_else(|errors| panic!("compile {name}: {errors:?}"));
    let path =
        std::env::temp_dir().join(format!("psrs-control-{name}-{}.wasm", std::process::id()));
    std::fs::write(&path, &artifact.wasm).expect("write component");
    let mut command = Command::new("wasmtime");
    command.arg("run");
    if force_gc {
        command
            .args(["-O", "gc-heap-initial-size=65536"])
            .env("WASMTIME_LOG", "wasmtime::runtime::vm::gc=trace");
    }
    let output = command.arg(&path).output().expect("run component");
    let _ = std::fs::remove_file(path);
    if force_gc {
        let log = String::from_utf8_lossy(&output.stderr);
        std::fs::write(
            std::env::temp_dir().join(format!("psrs-control-{name}-gc.log")),
            &output.stderr,
        )
        .expect("retain collection log");
        assert!(
            log.contains("After collection, GC heap's allocated bytes"),
            "no observed collection: {log}"
        );
    }
    assert_eq!(
        output.status.code(),
        Some(expected),
        "expected {name} to exit {expected}, got {output:?}"
    );
}
