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
