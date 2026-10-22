//! Source-level functional-dependency execution and negative checks (FE-15).
//!
//! A class with `a -> b` determines the `b` argument from `a`, so a use whose
//! determining argument is concrete does not need to annotate the determined
//! one. These programs execute under Wasmtime through the dictionary path.

use super::super::super::*;

mod nested_context;
mod rows;

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

#[test]
fn a_wildcard_in_a_fundep_determined_position_is_inferred() {
    // `passing/WildcardInInstance.purs`: a wildcard standing for a
    // fundep-determined argument is inferred. `purs` warns
    // `WildcardInferredType` with the solved type and accepts. It is not a
    // fundep conflict.
    let source = "\
module Main where

class Ask r m | m -> r where
  ask :: m r

instance askFun :: Ask r ((->) r) where
  ask x = x

test :: forall m. Ask _ m => m Int
test = ask
";
    let warnings =
        crate::check_program_with_warnings(&[("Main.purs", source)]).unwrap_or_else(|errors| {
            panic!("a fundep-determined wildcard is inferred, not a conflict: {errors:?}")
        });
    assert!(
        warnings.iter().any(|warning| {
            warning.diagnostic.code == Some("WildcardInferredType")
                && warning.diagnostic.message.contains("Int")
        }),
        "expected WildcardInferredType Int, got {warnings:?}"
    );
}

#[test]
fn a_type_wildcard_warns_with_its_inferred_type() {
    let source = "module Main where\n\
        test :: _ -> Int\n\
        test x = x\n\
        main :: Int\n\
        main = test 1\n";
    let warnings = crate::check_program_with_warnings(&[("Main.purs", source)])
        .unwrap_or_else(|errors| panic!("a solved type wildcard should be accepted: {errors:?}"));
    assert!(
        warnings.iter().any(|warning| {
            warning.diagnostic.code == Some("WildcardInferredType")
                && warning.diagnostic.message.contains("Int")
        }),
        "expected WildcardInferredType Int, got {warnings:?}"
    );
}

#[test]
fn official_fundep_wildcard_signature_typechecks_with_the_prelude() {
    // `passing/WildcardInInstance.purs`. The wildcard is inferred, so the
    // program type-checks. `purs` also warns `WildcardInferredType`.
    let source =
        include_str!("../../../../../../../tests/upstream/passing/WildcardInInstance.purs");
    crate::check_program_types_lenient_with_prelude(&[("Main.purs", source)]).unwrap_or_else(
        |errors| panic!("the fundep wildcard should be inferred, not rejected: {errors:?}"),
    );
}
