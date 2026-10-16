//! Partial application of an imported polymorphic function.
//!
//! A saturated call converts each argument to the callee declaration's
//! parameter shape. The adapter CC generates for a partial application has to
//! do the same for the parameters it leaves to the caller: a polymorphic
//! declaration stores its type-variable parameters erased, while the adapter's
//! parameter carries the callable type's concrete shape.

use super::*;

#[test]
fn a_partial_application_of_an_imported_polymorphic_function_converts_its_remaining_parameters() {
    // `flip` is polymorphic and imported. Without the conversion, the
    // generated call passes an `Int` where the declaration expects an erased
    // reference, and CC verification rejects the call.
    let source = r#"
module Main where

import Prelude
import Data.Function (flip)

combine :: Int -> Int -> Int
combine = flip (\a b -> a + b)

main = combine 20 22
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn a_fully_applied_imported_polymorphic_function_still_reaches_the_same_result() {
    // The saturated path is unchanged by the adapter fix.
    let source = r#"
module Main where

import Prelude
import Data.Function (flip)

main = flip (\a b -> a + b) 20 22
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn a_partial_application_of_a_local_closure_defers_the_remaining_argument() {
    // The callee is a local value, not a top-level declaration, so the
    // remaining parameter is exposed by a generated closure that calls the
    // captured callee indirectly.
    let source = r#"
module Main where

main :: Int
main =
  let combine = \a b -> intAdd a b
  in let step = combine 20
     in step 22
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn a_partial_application_of_a_dictionary_method_defers_the_remaining_argument() {
    // A class method reached through a field access is also an indirect
    // callee; its result still names the abstract constructor.
    let source = r#"
module Main where

class Chain f where
  chain :: forall a b. f a -> (a -> f b) -> f b

instance chainReader :: Chain ((->) Int) where
  chain m k x = k (m x) x

action :: Int -> Int
action x = x

run :: forall f. Chain f => f Int -> f Int
run a =
  let step = chain a
  in step (\_ -> a)

main :: Int
main = run action 42
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
