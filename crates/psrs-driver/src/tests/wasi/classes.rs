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

instance eqInt :: Eq Int where
  eq x y = true

instance ordInt :: Ord Int where
  compare x y = 42

lessThan :: forall a. Ord a => a -> a -> Int
lessThan x y = if eq x y then compare x y else 0

main :: Int
main = lessThan 1 2
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

const MULTI_PARAMETER_SOURCE: &str = r#"
module Main where

class Rel a b where
  rel :: a -> b -> Int

instance relIntBoolean :: Rel Int Boolean where
  rel x y = if y then x else 0

use :: forall a b. Rel a b => a -> b -> Int
use x y = rel x y

main :: Int
main = use 42 true
"#;

const INSTANCE_CONTEXT_SOURCE: &str = r#"
module Main where

class Eq a where
  eq :: a -> a -> Boolean

class ToInt a where
  toInt :: a -> Int

instance eqInt :: Eq Int where
  eq x y = true

instance toIntFromEq :: Eq a => ToInt a where
  toInt x = 1

main :: Int
main = toInt 42
"#;

const STRUCTURED_CONTEXT_SOURCE: &str = r#"
module Main where

class ToInt a where
  toInt :: a -> Int

data Box a = Box a

instance toIntInt :: ToInt Int where
  toInt x = x

instance toIntBox :: ToInt a => ToInt (Box a) where
  toInt b = case b of
    Box x -> toInt x

main :: Int
main = toInt (Box 42)
"#;

#[test]
fn a_multi_parameter_class_runs_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(MULTI_PARAMETER_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn an_instance_with_a_context_runs_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(INSTANCE_CONTEXT_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn a_structured_instance_context_runs_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(STRUCTURED_CONTEXT_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn a_superclass_method_runs_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(SUPERCLASS_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

const MULTIPLE_SUPERCLASS_SOURCE: &str = r#"
module Main where

class A a where
  a :: a -> Int

class B a where
  b :: a -> Int

class (A a, B a) <= C a where
  c :: a -> Int

instance aInt :: A Int where
  a x = 1

instance bInt :: B Int where
  b x = 2

instance cInt :: C Int where
  c x = a x

use :: forall x. C x => x -> Int
use x = a x

main :: Int
main = use 0
"#;

#[test]
fn a_multiple_superclass_class_runs_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(MULTIPLE_SUPERCLASS_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(1));
}

const SUPERCLASS_CYCLE_SOURCE: &str = r#"
module Main where

class A a <= B a where
  b :: a -> Int

class B a <= A a where
  a :: a -> Int

main :: Int
main = 0
"#;

const SUPERCLASS_ARGUMENT_SOURCE: &str = r#"
module Main where

class Eq a where
  eq :: a -> a -> Boolean

class Eq (Array a) <= Ord a where
  compare :: a -> a -> Int

main :: Int
main = 0
"#;

const AMBIGUOUS_CONTEXT_SOURCE: &str = r#"
module Main where

class Eq a where
  eq :: a -> a -> Boolean

class ToInt a where
  toInt :: a -> Int

instance toIntInt :: Eq b => ToInt Int where
  toInt x = 1

main :: Int
main = toInt 42
"#;

const INSTANCE_CHAIN_SOURCE: &str = r#"
module Main where

class ToInt a where
  toInt :: a -> Int

instance toIntInt :: ToInt Int where
  toInt x = x
else instance toIntBoolean :: ToInt Boolean where
  toInt x = 0

main :: Int
main = toInt 42
"#;

const FUNCTIONAL_DEPENDENCY_SOURCE: &str = r#"
module Main where

class Convert a b | a -> b where
  convert :: a -> b

main :: Int
main = 0
"#;

#[test]
fn a_superclass_cycle_is_reported() {
    let errors =
        compile_source("Main.purs", SUPERCLASS_CYCLE_SOURCE).expect_err("a cycle must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("class superclasses form a cycle")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn a_superclass_argument_outside_the_parameters_is_reported() {
    let errors = compile_source("Main.purs", SUPERCLASS_ARGUMENT_SOURCE)
        .expect_err("a foreign superclass argument must be rejected");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("a superclass argument must be one of the class's type parameters")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn an_ambiguous_instance_context_variable_is_reported() {
    let errors = compile_source("Main.purs", AMBIGUOUS_CONTEXT_SOURCE)
        .expect_err("an unbound context variable must be rejected");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("an instance context variable must appear in the instance head")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn an_instance_chain_is_reported_as_unsupported() {
    let errors = compile_source("Main.purs", INSTANCE_CHAIN_SOURCE)
        .expect_err("instance chains must be rejected");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("instance chains are not supported yet")),
        "unexpected diagnostics: {errors:?}"
    );
}

const CLASS_DEFAULT_SOURCE: &str = r#"
module Main where

class ToInt a where
  toInt :: a -> Int
  toInt x = 1

main :: Int
main = 1
"#;

const DERIVE_SOURCE: &str = r#"
module Main where

class ToInt a where
  toInt :: a -> Int

data Box = Box Int

derive instance toIntBox :: ToInt Box

main :: Int
main = 1
"#;

#[test]
fn a_class_default_is_reported_as_unsupported() {
    let errors = compile_source("Main.purs", CLASS_DEFAULT_SOURCE)
        .expect_err("class defaults must be rejected");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("class default implementations are not supported yet")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn a_deriving_declaration_is_reported_as_unsupported() {
    let errors = compile_source("Main.purs", DERIVE_SOURCE).expect_err("deriving must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("not supported yet")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn a_functional_dependency_is_reported_as_unsupported() {
    let errors = compile_source("Main.purs", FUNCTIONAL_DEPENDENCY_SOURCE)
        .expect_err("functional dependencies must be rejected");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("functional dependencies are not supported yet")),
        "unexpected diagnostics: {errors:?}"
    );
}
