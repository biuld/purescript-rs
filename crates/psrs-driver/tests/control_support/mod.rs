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
    let artifact = psrs_driver::compile_source("Main.purs", &fold_source(source))
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

/// Preserve coverage of user-defined open state folds on the revised primitive.
/// This is an ordinary library program, not a legacy compiler overload. Scoped
/// deep/shallow semantics are tested separately in control_scoped_runtime.rs.
pub fn fold_source(source: &str) -> String {
    if !source.contains("foldOpen") {
        return source.to_owned();
    }
    format!(
        "{source}\n{}",
        r"
foldOpen :: forall f g u a s. s -> (forall x. s -> f x -> (x -> C.Control u a) -> { _1 :: s, _2 :: C.Control u a }) -> (forall x r. u x -> (f x -> r) -> (g x -> r) -> r) -> (forall x. g x -> u x) -> C.Control u a -> C.Control g { _1 :: s, _2 :: a }
foldOpen state clause project inject computation = C.handle
  (\value -> C.return { _1: state, _2: value })
  (\request raw -> project request
    (\op -> let next = clause state op raw in foldOpen next._1 clause project inject next._2)
    (\op -> C.bind (C.perform op) (\answer -> foldOpen state clause project inject (raw answer))))
  computation
"
    )
}
