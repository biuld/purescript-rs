//! Mandatory execution evidence for the data-representation layouts. The test
//! compiles one representative program per GC layout family, encodes a core
//! component, and executes it under Wasmtime with a value-sensitive exit code.
//! `PSRS_REQUIRE_WASMTIME=1` makes the runtime mandatory.

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use psrs_driver::compile_source;

fn wasmtime_is_available() -> bool {
    let version = Command::new("wasmtime").arg("--version").output();
    let required = std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1");
    let Ok(version) = version else {
        if required {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
        }
        eprintln!("skipping: wasmtime is not installed");
        return false;
    };
    if !version.status.success() {
        if required {
            panic!("PSRS_REQUIRE_WASMTIME=1 but `wasmtime --version` failed: {version:?}");
        }
        eprintln!("skipping: wasmtime is unusable");
        return false;
    }
    true
}

fn assert_runs(source: &str, expected_code: i32) {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let artifact = compile_source("Main.purs", source).expect("compiling a layout fixture");
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "psrs-data-representation-{}-{id}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, artifact.wasm).expect("writing the layout artifact");
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("running the layout artifact");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "wasmtime could not execute the layout fixture: {output:?}"
    );
}

#[test]
fn executes_representative_reachable_layouts() {
    if !wasmtime_is_available() {
        return;
    }

    // Products and closed records plan as immutable structs.
    assert_runs(
        "module Main where\nmain = case { x: 40, y: 2 } of { x: a, y: b } -> a + b\n",
        42,
    );
    // Field-bearing sums plan as a tag-carrying supertype with final cases.
    assert_runs(
        "module Main where\ndata Shape = Rect Int | Dot\nmain = case Rect 42 of\n  Rect w -> w\n  Dot -> 0\n",
        42,
    );
    // All-nullary sums plan as immediate i32 tags with no object.
    assert_runs(
        "module Main where\ndata Color = Red | Green | Blue\nmain = case Green of\n  Green -> 42\n  _ -> 0\n",
        42,
    );
    // Arrays plan as mutable GC arrays; update is a pure clone.
    assert_runs("module Main where\nmain = arrayIndex [40, 42] 1\n", 42);
    // Closures plan as a code reference plus a nullable eqref capture array.
    assert_runs(
        "module Main where\nmain = let captured = 40 in let f = \\y -> captured + y in f 2\n",
        42,
    );
    // Erased parameterized fields box and recover exactly.
    assert_runs(
        "module Main where\ndata Maybe a = Nothing | Just a\nfromJust :: Maybe Int -> Int\nfromJust value = case value of\n  Just number -> number\n  _ -> 0\nmain = fromJust (Just 42)\n",
        42,
    );
    // Generic-to-concrete aggregate conversion reconstructs the array.
    assert_runs(
        "module Main where\nlastArr :: forall a. Int -> Array a -> Array a\nlastArr n x = if n == 0 then x else lastArr (n - 1) x\nmain = arrayIndex (lastArr 3 [40, 42]) 1\n",
        42,
    );
}
