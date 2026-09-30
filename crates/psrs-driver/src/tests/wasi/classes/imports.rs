//! Cross-module class instance selection (FE-14 vertical slice).
//!
//! A module that imports a class and its instances selects the imported
//! dictionary through the same solver path as a local instance. These tests
//! execute the linked two-module programs under Wasmtime and pin the negative
//! case where the instance's module is not imported.

use super::super::super::*;

const INSTANCE_LIBRARY_SOURCE: &str = r#"
module Lib where

class ToInt a where
  toInt :: a -> Int

instance toIntInt :: ToInt Int where
  toInt x = x
"#;

const INSTANCE_CONSUMER_SOURCE: &str = r#"
module Main where

import Lib

convert :: forall a. ToInt a => a -> Int
convert x = toInt x

main :: Int
main = convert 42
"#;

#[test]
fn selects_an_instance_from_an_imported_module_when_wasmtime_is_available() {
    let Some(output) = run_program_with_wasmtime(&[
        ("Lib.purs", INSTANCE_LIBRARY_SOURCE),
        ("Main.purs", INSTANCE_CONSUMER_SOURCE),
    ]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

const CONTEXT_LIBRARY_SOURCE: &str = r#"
module Lib where

class Eq a where
  eq :: a -> a -> Boolean

class ToInt a where
  toInt :: a -> Int

instance eqInt :: Eq Int where
  eq x y = true

instance toIntFromEq :: Eq Int => ToInt Int where
  toInt x = 1
"#;

const CONTEXT_CONSUMER_SOURCE: &str = r#"
module Main where

import Lib

main :: Int
main = toInt 42
"#;

const POLYMORPHIC_LIBRARY_SOURCE: &str = r#"
module Lib where

class Eq a where
  eq :: a -> a -> Boolean

class ToInt a where
  toInt :: a -> Int

instance eqInt :: Eq Int where
  eq x y = true

instance toIntFromEq :: Eq a => ToInt a where
  toInt x = 1
"#;

const POLYMORPHIC_CONSUMER_SOURCE: &str = r#"
module Main where

import Lib

main :: Int
main = toInt 42
"#;

#[test]
fn selects_a_polymorphic_imported_instance_when_wasmtime_is_available() {
    let Some(output) = run_program_with_wasmtime(&[
        ("Lib.purs", POLYMORPHIC_LIBRARY_SOURCE),
        ("Main.purs", POLYMORPHIC_CONSUMER_SOURCE),
    ]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

const POLY_SUPER_LIBRARY_SOURCE: &str = r#"
module Lib where

class Eq a where
  eq :: a -> a -> Boolean

class Eq a <= Ord a where
  compare :: a -> a -> Int

instance eqInt :: Eq Int where
  eq x y = true

instance ordFromEq :: Eq a => Ord a where
  compare x y = 42
"#;

const POLY_SUPER_CONSUMER_SOURCE: &str = r#"
module Main where

import Lib

main :: Int
main = compare 1 2
"#;

#[test]
fn projects_a_polymorphic_imported_superclass_when_wasmtime_is_available() {
    let Some(output) = run_program_with_wasmtime(&[
        ("Lib.purs", POLY_SUPER_LIBRARY_SOURCE),
        ("Main.purs", POLY_SUPER_CONSUMER_SOURCE),
    ]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn selects_an_imported_instance_whose_context_solves_remotely_when_wasmtime_is_available() {
    let Some(output) = run_program_with_wasmtime(&[
        ("Lib.purs", CONTEXT_LIBRARY_SOURCE),
        ("Main.purs", CONTEXT_CONSUMER_SOURCE),
    ]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(1));
}

const SUPERCLASS_LIBRARY_SOURCE: &str = r#"
module Lib where

class Eq a where
  eq :: a -> a -> Boolean

class Eq a <= Ord a where
  compare :: a -> a -> Int

instance eqInt :: Eq Int where
  eq x y = true

instance ordInt :: Ord Int where
  compare x y = 42
"#;

const SUPERCLASS_CONSUMER_SOURCE: &str = r#"
module Main where

import Lib

lessThan :: forall a. Ord a => a -> a -> Int
lessThan x y = if eq x y then compare x y else 0

main :: Int
main = lessThan 1 2
"#;

#[test]
fn projects_a_superclass_from_an_imported_instance_when_wasmtime_is_available() {
    let Some(output) = run_program_with_wasmtime(&[
        ("Lib.purs", SUPERCLASS_LIBRARY_SOURCE),
        ("Main.purs", SUPERCLASS_CONSUMER_SOURCE),
    ]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

const CLASS_ONLY_SOURCE: &str = r#"
module Classes where

class ToInt a where
  toInt :: a -> Int
"#;

const ORPHAN_INSTANCE_SOURCE: &str = r#"
module Instances where

import Classes

instance toIntInt :: ToInt Int where
  toInt x = x
"#;

const UNIMPORTED_CONSUMER_SOURCE: &str = r#"
module Main where

import Classes

main :: Int
main = toInt 42
"#;

#[test]
fn an_instance_from_an_unimported_module_is_unavailable() {
    let errors = compile_program_sources(&[
        ("Classes.purs", CLASS_ONLY_SOURCE),
        ("Instances.purs", ORPHAN_INSTANCE_SOURCE),
        ("Main.purs", UNIMPORTED_CONSUMER_SOURCE),
    ])
    .expect_err("an instance declared in an unimported module must not be selected");
    assert!(
        errors.iter().any(|error| error
            .diagnostic
            .message
            .contains("no instance for constraint ToInt Int")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn adapts_function_arrays_in_imported_generic_records_when_wasmtime_is_available() {
    let library = r#"
module Lib where

make :: forall a. a -> { functions :: Array (a -> a) }
make value = { functions: [\x -> x] }

nested :: forall a. a -> { groups :: Array { functions :: Array (a -> a) } }
nested value = { groups: [make value] }

apply :: forall a. { functions :: Array (a -> a) } -> a -> a
apply record value = (arrayIndex (record.functions) 0) value

data Wrap a = Wrap (Array (a -> a))

wrap :: forall a. a -> Wrap a
wrap value = Wrap [\x -> x]

unwrap :: forall a. Wrap a -> Array (a -> a)
unwrap value = case value of
  Wrap functions -> functions
"#;
    for body in [
        "let record = make 42 in (arrayIndex (record.functions) 0) 42",
        "let record = nested 42 in let group = arrayIndex (record.groups) 0 in (arrayIndex (group.functions) 0) 42",
        "apply { functions: [\\x -> x] } 42",
        "let functions = unwrap (wrap 42) in (arrayIndex functions 0) 42",
    ] {
        let consumer = format!("module Main where\nimport Lib\nmain :: Int\nmain = {body}\n");
        let Some(output) =
            run_program_with_wasmtime(&[("Lib.purs", library), ("Main.purs", &consumer)])
        else {
            assert_ne!(std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(), Ok("1"));
            eprintln!("skipping: wasmtime is not installed");
            return;
        };
        assert_eq!(output.status.code(), Some(42), "{body}: {output:?}");
    }
}
