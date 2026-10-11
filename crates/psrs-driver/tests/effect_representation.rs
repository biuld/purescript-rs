//! Prototype acceptance for the library-owned algebraic-effects representation.
//!
//! Before the native handler mechanism selected by
//! [DEC-19](../../../docs/decision/DEC-19-compiler-generated-resumable-frames.md)
//! is lowered, the [algebraic-effects design](../../../docs/design/backend/fp/algebraic-effects-and-handlers.md)
//! requires validating the shared generic representation: a rank-N
//! heterogeneous request transport and the common computation type the official
//! Effect/ST facades share. This test pins that representation and its pure
//! interpretation. It does not claim the compiler-generated resumable-frame
//! mechanism, and it is not standard-library migration evidence.

use std::process::Command;

/// A rank-N eliminator whose method is itself rank-1 polymorphic transports a
/// typed payload and continuation. `Effect` and `ST` wrap the same `Free` field,
/// and `toEffect = unsafeCoerce` is the common Global conversion.
const COMMON_REPRESENTATION: &str = r#"module Main where

import Unsafe.Coerce (unsafeCoerce)

data Request next = Request
  (forall z. { run :: forall a. a -> (a -> next) -> z } -> z)

data Free a = Pure a | Step (Request (Free a))

newtype Effect a = Effect (Free a)
newtype ST a = ST (Free a)

toEffect :: forall a. ST a -> Effect a
toEffect = unsafeCoerce

mk :: forall a. a -> Request (Free a)
mk value = Request \ops -> ops.run value Pure

step :: forall a. a -> Free a
step value = Step (mk value)

interp :: forall a. Free a -> a
interp (Pure a) = a
interp (Step k) = case k of
  Request f -> interp (f { run: \x next -> next x })

stProg :: ST Int
stProg = ST (step 42)

main :: Int
main = case toEffect stProg of
  Effect f -> interp f
"#;

/// A request transports its payload and continuation at one result type; using
/// the request itself where its result type is wanted must be rejected.
const REQUEST_IS_NOT_ITS_RESULT: &str = r#"module Main where

data Request next = Request
  (forall z. { run :: forall a. a -> (a -> next) -> z } -> z)

data Free a = Pure a | Step (Request (Free a))

mk :: forall a. a -> Request (Free a)
mk value = Request \ops -> ops.run value Pure

bad :: Int
bad = Step (mk 42)
"#;

#[test]
fn common_representation_checks() {
    if let Err(errors) = psrs_driver::check_source("Main.purs", COMMON_REPRESENTATION) {
        panic!("common representation rejected: {errors:?}");
    }
}

#[test]
fn a_request_is_not_interchangeable_with_its_result() {
    assert!(
        psrs_driver::check_source("Main.purs", REQUEST_IS_NOT_ITS_RESULT).is_err(),
        "a request used as its own result type must be rejected"
    );
}

#[test]
fn executes_a_pure_handler_over_the_shared_representation() {
    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert_ne!(
            std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(),
            Ok("1"),
            "the pure-handler execution requires wasmtime"
        );
        eprintln!("skipping effect-representation execution: wasmtime is unavailable");
        return;
    }
    let artifact = psrs_driver::compile_source("Main.purs", COMMON_REPRESENTATION)
        .expect("compile the common representation");
    let path = std::env::temp_dir().join(format!(
        "psrs-effect-representation-{}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, &artifact.wasm).expect("write component");
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("run component");
    let _ = std::fs::remove_file(path);
    assert_eq!(
        output.status.code(),
        Some(42),
        "expected the pure handler to return 42, got {output:?}"
    );
}
