//! Frontend acceptance for the compiler-owned delimited-control surface.
//!
//! `Prim.Control` exposes the effect-agnostic `Control f a` carrier with
//! `return`/`bind`/`perform`/`run`/`handle` ([DEC-19](../../../docs/decision/DEC-19-compiler-generated-resumable-frames.md),
//! [delimited control and resumable frames](../../../docs/design/backend/fp/delimited-control-and-resumable-frames.md)).
//! Source acceptance/rejection and focused executor execution.
//! Open execution is covered in `control_open_runtime.rs`.

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
handled = foldOpen 0 handler project inject program

main :: Int
main = 0
"#;

mod control_support;
use control_support::execute;

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
    execute("return", RETURNS_AND_RUNS, 42);
}

#[test]
fn executes_a_performed_request() {
    execute("perform", &ACCEPTS.replace("C.run 0", "C.run 42"), 42);
}

#[test]
fn control_surface_handles_one_effect() {
    if let Err(errors) = psrs_driver::check_source(
        "Main.purs",
        &control_support::fold_source(HANDLES_ONE_EFFECT),
    ) {
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

#[test]
fn open_control_rejects_an_incompatible_residual_functor() {
    let source = HANDLES_ONE_EFFECT.replace("handled :: C.Control G", "handled :: C.Control F");
    assert!(
        psrs_driver::check_source("Main.purs", &control_support::fold_source(&source)).is_err()
    );
}

#[test]
fn library_fold_rejects_an_incompatible_injection() {
    let source = HANDLES_ONE_EFFECT.replace(
        "inject :: forall x. G x -> U x\ninject = UG",
        "inject :: forall x. F x -> U x\ninject = UF",
    );
    assert!(
        psrs_driver::check_source("Main.purs", &control_support::fold_source(&source)).is_err()
    );
}

#[test]
fn library_fold_requires_a_rank_two_projection() {
    let source = HANDLES_ONE_EFFECT.replace(
        "project :: forall x r. U x -> (F x -> r) -> (G x -> r) -> r",
        "project :: forall r. U Int -> (F Int -> r) -> (G Int -> r) -> r",
    );
    assert!(
        psrs_driver::check_source("Main.purs", &control_support::fold_source(&source)).is_err()
    );
}
