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
import Prelude

class ToInt a where
  toInt :: a -> Int

instance toIntInt :: ToInt Int where
  toInt x = x + 1

main :: Int
main = toInt 41
"#;

const POLYMORPHIC_METHOD_SOURCE: &str = r#"
module Main where

class Keep a where
  keep :: forall b. a -> b -> a

instance keepInt :: Keep Int where
  keep value _ = value

main :: Int
main = keep (keep 42 "ignored") true
"#;

#[test]
fn polymorphic_class_methods_instantiate_independently_at_each_use() {
    let Some(output) = run_with_wasmtime(POLYMORPHIC_METHOD_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn rejects_an_instance_that_specializes_a_polymorphic_class_method() {
    let source = r#"module Main where

class Keep a where
  keep :: forall b. a -> b -> a

instance keepInt :: Keep Int where
  keep value flag = if flag then value else value

main :: Int
main = keep 42 true
"#;
    let errors = compile_source("Main.purs", source)
        .expect_err("an instance must implement every method forall polymorphically");
    assert!(
        errors.iter().any(|error| {
            error.message.contains("signature mismatch") || error.message.contains("type mismatch")
        }),
        "unexpected diagnostics: {errors:?}"
    );
}

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

class Eq a <= Ord b where
  compare :: b -> Int

main :: Int
main = 0
"#;

/// A superclass edge is written over the subclass's own parameters, so
/// `class Super (Array a) <= Sub a` elaborates and `use` finds `Super (Array
/// Int)` through the edge. The edge's argument is a type, not a name, so
/// nothing in dictionary construction or superclass search matches a parameter
/// name to find it.
const SUPERCLASS_CONSTRUCTED_ARGUMENT_SOURCE: &str = r#"
module Main where
import Prelude

class Gamma a where
  gamma :: a -> Int

class (Gamma (Array a)) <= Epsilon a where
  size :: Array a -> Int

instance gammaIntArray :: Gamma (Array Int) where
  gamma _ = 7

instance epsilonInt :: Epsilon Int where
  size _ = 1

use :: forall a. Epsilon a => Array a -> Int
use xs = gamma xs + size xs

main :: Int
main = use [1, 2, 3]
"#;

#[test]
fn a_superclass_edge_over_a_constructed_argument_runs_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(SUPERCLASS_CONSTRUCTED_ARGUMENT_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(8));
}

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

/// A superclass argument must be written over the class's own parameters. The
/// kind layer owns name binding and reports the free `a` first; the type checker
/// keeps the same rule for itself, as an `UnsupportedClass` diagnostic on the
/// offending argument, so a superclass template can never mention a variable
/// outside its binder scope.
#[test]
fn a_superclass_argument_outside_the_class_scope_is_reported() {
    let errors = compile_source("Main.purs", SUPERCLASS_ARGUMENT_SOURCE)
        .expect_err("a foreign superclass argument must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("the type variable `a` is undefined")),
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
fn instance_context_variables_determined_by_functional_dependencies_are_accepted() {
    let source = r#"
module Main where

class KeyValue key value | key -> value
class Container key

instance containerKeyValue :: KeyValue key value => Container key
"#;
    crate::typecheck_program_sources(&[("Main.purs", source)])
        .expect("the instance head determines `value` through KeyValue's functional dependency");
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
fn a_class_method_body_is_rejected_as_invalid_purescript() {
    let errors = compile_source("Main.purs", CLASS_DEFAULT_SOURCE)
        .expect_err("class defaults must be rejected");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("class bodies permit method signatures only")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn a_deriving_declaration_is_reported_as_unsupported() {
    let errors = compile_source("Main.purs", DERIVE_SOURCE).expect_err("deriving must be rejected");
    assert!(
        errors.iter().any(|error| {
            error
                .message
                .contains("known-class deriving rule is unavailable")
        }),
        "unexpected diagnostics: {errors:?}"
    );
}

mod deriving;
mod fundeps;
mod imports;
mod instance_chains;
mod instance_signatures;
mod polymorphic_methods;
