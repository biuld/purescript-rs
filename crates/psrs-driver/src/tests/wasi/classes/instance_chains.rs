//! Ordered instance-chain selection tests (FE-14).

use super::super::super::*;

const IMPORTED_CHAIN_LIBRARY: &str = r#"
module Lib where

class Need a where
  need :: a -> Int

instance needBoolean :: Need Boolean where
  need _ = 0

instance needInt :: Need Int where
  need _ = 0

class ToInt a where
  toInt :: a -> Int

instance toIntBoolean :: ToInt Boolean where
  toInt _ = 1
else instance toIntFallback :: Need a => ToInt a where
  toInt _ = 41
"#;

const IMPORTED_CHAIN_CONSUMER: &str = r#"
module Main where

import Lib

main :: Int
main = intAdd (toInt true) (toInt 0)
"#;

#[test]
fn an_imported_instance_chain_preserves_source_order_when_wasmtime_is_available() {
    let Some(output) = run_program_with_wasmtime(&[
        ("Lib.purs", IMPORTED_CHAIN_LIBRARY),
        ("Main.purs", IMPORTED_CHAIN_CONSUMER),
    ]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

const FUNDEP_CHAIN_SOURCE: &str = r#"
module Main where

class Select a b | a -> b where
  choose :: a -> Int

instance selectInt :: Select Int Boolean where
  choose _ = 42
else instance selectFallback :: Select a Int where
  choose _ = 0

main :: Int
main = choose 1
"#;

#[test]
fn fundep_improvement_uses_only_the_selected_chain_branch_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(FUNDEP_CHAIN_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

const INDEPENDENT_ARGUMENT_APARTNESS: &str = r#"
module Main where

class Select a b c | a -> b where
  choose :: a -> c -> Int

instance first :: Select Int Boolean Int where
  choose _ _ = 0
else instance fallback :: Select a Int c where
  choose _ _ = 42

main :: Int
main = choose 1 true
"#;

#[test]
fn fundep_branch_matching_still_checks_independent_class_arguments_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(INDEPENDENT_ARGUMENT_APARTNESS) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

const TRANSITIVE_FUNDEP_CHAIN: &str = r#"
module Main where

class Select a b c | a -> b, b -> c where
  choose :: a -> Int

instance first :: Select Int Boolean Number where
  choose _ = 42
else instance fallback :: Select Int String Char where
  choose _ = 0

main :: Int
main = choose 1
"#;

#[test]
fn fundep_chain_matching_uses_transitive_determining_closure_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(TRANSITIVE_FUNDEP_CHAIN) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

const RECURSIVE_APPLICATION_CHAIN: &str = r#"
module Main where

class Arg i o | i -> o where
  arg :: i -> Int

instance appArg :: Arg i o => Arg (f i) o where
  arg _ = 42
else instance reflArg :: Arg a a where
  arg _ = 0

identity :: Int -> Int
identity x = x

main :: Int
main = arg identity
"#;

#[test]
fn recursive_application_head_solves_its_selected_branch_context_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(RECURSIVE_APPLICATION_CHAIN) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

const OCCURS_CHECK_CHAIN: &str = r#"
module Main where

class Same a b where
  same :: a -> b -> Int

instance repeated :: Same a a where
  same _ _ = 0
else instance fallback :: Same a b where
  same _ _ = 42

use :: forall a. a -> (a -> Int) -> Int
use x xs = same x xs

main :: Int
main = use 1 (\x -> x)
"#;

#[test]
fn repeated_head_variable_with_an_occurs_conflict_is_apart_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(OCCURS_CHECK_CHAIN) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

const UNKNOWN_HEAD_BLOCKS_FALLBACK: &str = r#"
module Main where

class Same a b where
  same :: a -> b -> Int

instance sameInt :: Same Int Int where
  same _ _ = 1
else instance sameFallback :: Same a b where
  same _ _ = 2

use :: forall a. a -> Int
use x = same x 0

main :: Int
main = use true
"#;

#[test]
fn an_unknown_earlier_head_blocks_later_chain_branches() {
    let errors = compile_source("Main.purs", UNKNOWN_HEAD_BLOCKS_FALLBACK)
        .expect_err("an unknown earlier branch must block the fallback");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("no instance")),
        "unexpected diagnostics: {errors:?}"
    );
}

const CONTEXT_FAILURE_DOES_NOT_FALL_THROUGH: &str = r#"
module Main where

class Need a where
  need :: a -> Int

class Choose a where
  choose :: a -> Int

instance chooseNeedsEvidence :: Need a => Choose a where
  choose x = need x
else instance chooseInt :: Choose Int where
  choose _ = 42

main :: Int
main = choose 1
"#;

#[test]
fn a_selected_branch_context_failure_does_not_fall_through() {
    let errors = compile_source("Main.purs", CONTEXT_FAILURE_DOES_NOT_FALL_THROUGH)
        .expect_err("context failure must not select a later branch");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("no instance for constraint Need Int")),
        "unexpected diagnostics: {errors:?}"
    );
}

const ORDINARY_OVERLAP: &str = r#"
module Main where

class Mark a where
  mark :: a -> Int

instance markAny :: Mark a where
  mark _ = 1

instance markInt :: Mark Int where
  mark _ = 2

main :: Int
main = mark 0
"#;

#[test]
fn an_overlapping_ordinary_instance_is_not_resolved_by_declaration_order() {
    let errors = compile_source("Main.purs", ORDINARY_OVERLAP)
        .expect_err("ordinary overlapping instances must remain incoherent");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("overlapping instances")),
        "unexpected diagnostics: {errors:?}"
    );
}

const ORPHAN_ELSE: &str = r#"
module Main where

else instance invalid :: Missing Int where
  missing _ = 0

main :: Int
main = 0
"#;

#[test]
fn an_else_instance_without_a_preceding_branch_is_rejected() {
    let errors =
        compile_source("Main.purs", ORPHAN_ELSE).expect_err("an else branch cannot start a chain");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("must follow an instance in the same chain")),
        "unexpected diagnostics: {errors:?}"
    );
}

const CHAIN_CLASS_MISMATCH: &str = r#"
module Main where

class A a where
  a :: a -> Int

class B a where
  b :: a -> Int

instance aInt :: A Int where
  a x = x
else instance bInt :: B Int where
  b x = x

main :: Int
main = 0
"#;

#[test]
fn branches_in_one_chain_must_name_the_same_class() {
    let errors = compile_source("Main.purs", CHAIN_CLASS_MISMATCH)
        .expect_err("a chain cannot change its class");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("one instance chain contains different classes")),
        "unexpected diagnostics: {errors:?}"
    );
}
