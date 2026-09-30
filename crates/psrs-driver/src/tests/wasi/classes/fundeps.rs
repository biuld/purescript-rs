//! Source-level functional-dependency execution and negative checks (FE-15).
//!
//! A class with `a -> b` determines the `b` argument from `a`, so a use whose
//! determining argument is concrete does not need to annotate the determined
//! one. These programs execute under Wasmtime through the dictionary path.

use super::super::super::*;

const FUNCTIONAL_DEPENDENCY_SOURCE: &str = r#"
module Main where

class Produce a b | a -> b where
  produce :: a -> b

instance produceInt :: Produce Int Int where
  produce x = x

discard :: forall a. a -> Int
discard _ = 42

main :: Int
main = discard (produce 42)
"#;

const FUNCTIONAL_DEPENDENCY_METHOD_SOURCE: &str = r#"
module Main where

class Peek a b | a -> b where
  peek :: a -> Int
  describe :: a -> b

instance peekInt :: Peek Int Boolean where
  peek _ = 42
  describe _ = true

main :: Int
main = peek 0
"#;

const AMBIGUOUS_FUNCTIONAL_DEPENDENCY_SOURCE: &str = r#"
module Main where

class Peek a b where
  peek :: a -> Int

instance peekInt :: Peek Int b where
  peek _ = 1

main = peek 42
"#;

const FUNDEP_CONFLICT_SOURCE: &str = r#"
module Main where

class Produce a b | a -> b where
  produce :: a -> b

instance produceIntInt :: Produce Int Int where
  produce x = x

instance produceIntBoolean :: Produce Int Boolean where
  produce x = true

main :: Int
main = produce 42
"#;

const UNKNOWN_FUNDEP_SOURCE: &str = r#"
module Main where

class Bogus a b | a -> c where
  bogus :: a -> b

main :: Int
main = 0
"#;

const FUNCTIONAL_DEPENDENCY_GIVEN_SOURCE: &str = r#"
module Main where

class Produce a b | a -> b where
  produce :: a -> b

instance produceInt :: Produce Int Int where
  produce x = x

discard :: forall a. a -> Int
discard _ = 42

useGiven :: forall a b. Produce a b => a -> Int
useGiven x = discard (produce x)

main :: Int
main = useGiven 10
"#;

#[test]
fn a_functional_dependency_determines_the_result_type_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(FUNCTIONAL_DEPENDENCY_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn a_functional_dependency_selects_a_dictionary_method_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(FUNCTIONAL_DEPENDENCY_METHOD_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn a_functional_dependency_improves_from_a_given_constraint_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(FUNCTIONAL_DEPENDENCY_GIVEN_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn an_undetermined_functional_dependency_constraint_is_reported_as_ambiguous() {
    let errors = compile_source("Main.purs", AMBIGUOUS_FUNCTIONAL_DEPENDENCY_SOURCE)
        .expect_err("an undetermined constraint must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("ambiguous constraint")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn conflicting_functional_dependency_positions_are_reported() {
    let errors = compile_source("Main.purs", FUNDEP_CONFLICT_SOURCE)
        .expect_err("conflicting determined positions must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("functional dependency conflict")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn an_unknown_functional_dependency_variable_is_reported() {
    let errors = compile_source("Main.purs", UNKNOWN_FUNDEP_SOURCE)
        .expect_err("a fundep variable outside the parameters must be rejected");
    assert!(
        errors.iter().any(|error| error.message.contains(
            "a functional dependency variable must be one of the class's type parameters"
        )),
        "unexpected diagnostics: {errors:?}"
    );
}
