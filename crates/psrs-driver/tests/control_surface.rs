//! Frontend acceptance for the compiler-owned delimited-control surface.
//!
//! `Prim.Control` exposes the effect-agnostic `Control f a` carrier with
//! `return`/`bind`/`perform`/`run`/`handle` ([DEC-19](../../../docs/decision/DEC-19-compiler-generated-resumable-frames.md),
//! [delimited control and resumable frames](../../../docs/design/backend/fp/delimited-control-and-resumable-frames.md)).
//! The selective CPS / resumable-frame lowering is not implemented, so these
//! tests check source acceptance and rejection (P0-P5) only; they do not claim
//! execution.

/// A well-typed program over an opaque request functor, interpreted by the
/// closed state-threading `run`. `Op`'s answer type is `Int`, and the handler
/// threads an `Int` state, so `run 0 handler program` is `{ _1 :: Int, _2 :: Int }`.
const ACCEPTS: &str = r#"module Main where

import Prim.Control as C

data Op next = Op (Int -> next)

program :: C.Control Op Int
program = C.perform (Op \x -> x)

runProgram :: { _1 :: Int, _2 :: Int }
runProgram = C.run 0 handler program
  where
  handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
  handler s (Op k) cont = { _1: s, _2: cont (k s) }

main :: Int
main = runProgram._2
"#;

/// A `Control f a` computation is not interchangeable with its result type.
const REJECTS_RESULT_TYPE: &str = r#"module Main where

import Prim.Control as C

data Op next = Op (Int -> next)

program :: C.Control Op Int
program = C.perform (Op \x -> x)

bad :: Int
bad = program
"#;

/// `run`'s handler must be rank-2: a handler specialized to one answer type
/// cannot satisfy the polymorphic argument.
const REJECTS_MONOMORPHIC_HANDLER: &str = r#"module Main where

import Prim.Control as C

data Op next = Op (Int -> next)

program :: C.Control Op Int
program = C.perform (Op \x -> x)

mono :: Int -> Op Int -> (Int -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
mono s (Op k) cont = { _1: s, _2: cont (k s) }

bad :: { _1 :: Int, _2 :: Int }
bad = C.run 0 mono program
"#;

/// The open eliminator removes one effect: `handle` interprets the `F` requests
/// of a computation over the combined effect `U` and leaves the `G` requests as a
/// residual `Control G`.
const HANDLES_ONE_EFFECT: &str = r#"module Main where

import Prim.Control as C

data F next = F (Int -> next)
data G next = G (Int -> next)
data U next = UF (F next) | UG (G next)

project :: forall x r. U x -> (F x -> r) -> (G x -> r) -> r
project u onF onG = case u of
  UF f -> onF f
  UG g -> onG g

inject :: forall x. G x -> U x
inject = UG

handler :: forall x. Int -> F x -> (x -> C.Control U Int) -> { _1 :: Int, _2 :: C.Control U Int }
handler s (F k) cont = { _1: s, _2: cont (k s) }

program :: C.Control U Int
program = C.bind (C.perform (UF (F \x -> x))) \_ -> C.perform (UG (G \x -> x))

handled :: C.Control G { _1 :: Int, _2 :: Int }
handled = C.handle 0 handler project inject program

main :: Int
main = 0
"#;

use std::process::Command;

/// The `return`-only path lowers to a frame and executes: `main` reads the
/// `(state, value)` result of the closed `run`.
const RETURNS_AND_RUNS: &str = r#"module Main where

import Prim.Control as C

data Op next = Op (Int -> next)

program :: C.Control Op Int
program = C.return 42

result :: { _1 :: Int, _2 :: Int }
result = C.run 0 handler program
  where
  handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
  handler s (Op k) cont = { _1: s, _2: cont (k s) }

main :: Int
main = result._2
"#;

#[test]
fn control_surface_typechecks() {
    if let Err(errors) = psrs_driver::check_source("Main.purs", ACCEPTS) {
        panic!("control surface rejected: {errors:?}");
    }
}

#[test]
fn executes_a_returning_computation() {
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
    let artifact = psrs_driver::compile_source("Main.purs", RETURNS_AND_RUNS)
        .expect("compile the returning computation");
    let path =
        std::env::temp_dir().join(format!("psrs-control-return-{}.wasm", std::process::id()));
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
        "expected the returning computation to exit 42, got {output:?}"
    );
}

#[test]
fn control_surface_handles_one_effect() {
    if let Err(errors) = psrs_driver::check_source("Main.purs", HANDLES_ONE_EFFECT) {
        panic!("open handle rejected: {errors:?}");
    }
}

#[test]
fn control_surface_rejects_result_type_confusion() {
    assert!(
        psrs_driver::check_source("Main.purs", REJECTS_RESULT_TYPE).is_err(),
        "a computation used as its own result type must be rejected"
    );
}

#[test]
fn control_surface_requires_a_rank_two_handler() {
    assert!(
        psrs_driver::check_source("Main.purs", REJECTS_MONOMORPHIC_HANDLER).is_err(),
        "a monomorphic handler must not satisfy run's rank-2 argument"
    );
}
