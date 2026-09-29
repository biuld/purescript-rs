//! Source-level class and instance execution (FE-14 vertical slice).
//!
//! These tests compile ordinary source programs that declare a class, an
//! instance, and a constrained function, then execute the result under
//! Wasmtime using the backend dictionary path.

use super::super::*;

const CONSTRAINED_FUNCTION_SOURCE: &str = r#"
module Main where

class ToInt a where
  toInt :: a -> Int

instance toIntInt :: ToInt Int where
  toInt x = x

convert :: forall a. ToInt a => a -> Int
convert x = toInt x

main :: Int
main = convert 42
"#;

const DIRECT_METHOD_SOURCE: &str = r#"
module Main where

class ToInt a where
  toInt :: a -> Int

instance toIntInt :: ToInt Int where
  toInt x = x + 1

main :: Int
main = toInt 41
"#;

#[test]
fn constrained_function_selects_an_instance_dictionary_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(CONSTRAINED_FUNCTION_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn method_call_selects_an_instance_dictionary_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(DIRECT_METHOD_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

const MISSING_INSTANCE_SOURCE: &str = r#"
module Main where

class ToInt a where
  toInt :: a -> Int

instance toIntBool :: ToInt Boolean where
  toInt x = 0

convert :: forall a. ToInt a => a -> Int
convert x = toInt x

main :: Int
main = convert 42
"#;

const SUPERCLASS_SOURCE: &str = r#"
module Main where

class Eq a where
  eq :: a -> a -> Boolean

class Eq a <= Ord a where
  compare :: a -> a -> Int

main :: Int
main = 0
"#;

#[test]
fn an_unresolved_constraint_is_reported() {
    let errors = compile_source("Main.purs", MISSING_INSTANCE_SOURCE)
        .expect_err("missing instance must be rejected");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("no instance for constraint ToInt Int")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn a_superclass_class_is_reported_as_unsupported() {
    let errors =
        compile_source("Main.purs", SUPERCLASS_SOURCE).expect_err("superclasses must be rejected");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("class superclasses are not supported yet")),
        "unexpected diagnostics: {errors:?}"
    );
}
