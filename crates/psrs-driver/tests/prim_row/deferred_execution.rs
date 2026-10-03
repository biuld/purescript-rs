//! Runtime evidence that an inferred `Union` residual retained through an
//! instance context supplies its dictionary when the inferred function is
//! instantiated, and that the selected instance method still executes.

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use psrs_driver::compile_source;

const SOURCE: &str = r#"module Main where
import Prim
import Prim.Row (class Union)

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

extend :: forall row. Proxy row -> Proxy (y :: Int | row)
extend _ = Proxy

class Joined left right output where
  joined :: Proxy left -> Proxy right -> Proxy output -> Int

instance joinedUnion :: Union left right output => Joined left right output where
  joined _ _ _ = 42

nested left right output = joined (extend left) right output

main :: Int
main = nested
  (Proxy :: Proxy (a :: Int))
  (Proxy :: Proxy (b :: Boolean))
  (Proxy :: Proxy (y :: Int | (a :: Int | (b :: Boolean))))
"#;

#[test]
fn executes_a_method_through_an_inferred_nested_union_residual() {
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

    let artifact = compile_source("Main.purs", SOURCE)
        .expect("the inferred nested Union context must compile");
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "psrs-row-deferred-{}-{id}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, artifact.wasm).expect("write row deferral artifact");
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("run row deferral artifact");
    let _ = std::fs::remove_file(path);
    assert_eq!(
        output.status.code(),
        Some(42),
        "the inferred class method did not execute with its instance evidence: {output:?}"
    );
}
