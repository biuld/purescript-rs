//! Explicit instance method signatures are checked before dictionary lowering.

use super::super::super::*;

const SIGNED_INSTANCE_SOURCE: &str = r#"
module Main where

class Value a where
  value :: a

instance valueNumber :: Value Number where
  value :: Number
  value = 42.0

class Equal a where
  equal :: a -> a -> Boolean

instance equalNumber :: Equal Number where
  equal :: forall x y. x -> y -> Boolean
  equal _ _ = true

main :: Int
main = if equal value 42.0 then 42 else 0
"#;

#[test]
fn a_constrained_member_annotation_executes_when_wasmtime_is_available() {
    let source = r#"
module Main where

class Eq a where
  eq :: a -> a -> Boolean

instance eqBoolean :: Eq Boolean where
  eq _ _ = true

class Flag a where
  flag :: a -> Int

instance flagBoolean :: Flag Boolean where
  flag :: forall a. Eq a => a -> Int
  flag _ = 42

main :: Int
main = flag true
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn specialized_and_general_instance_signatures_execute_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(SIGNED_INSTANCE_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn explicit_instance_signatures_reject_mismatched_types() {
    let cases = [("Boolean", "true"), ("Int", "0.0"), ("Number", "true")];
    for (signature, body) in cases {
        let source = format!(
            "module Main where\n\
class Value a where\n  value :: a\n\
instance valueNumber :: Value Number where\n  value :: {signature}\n  value = {body}\n"
        );
        let errors = check_program_types_lenient(&[("Main.purs", &source)])
            .expect_err("an invalid instance signature or body must be rejected");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.code == Some("TypesDoNotUnify")),
            "unexpected diagnostics for `{signature}` / `{body}`: {errors:?}"
        );
    }
}

#[test]
fn instance_signatures_resolve_type_variables_from_the_instance_head() {
    let source = r#"
module Main where

class Size a where
  size :: a -> Int

data Box a = Box a

instance sizeBox :: Size (Box item) where
  size :: Box item -> Int
  size (Box _) = 1
"#;
    check_program_types_lenient(&[("Main.purs", source)])
        .expect("an instance signature may refer to its head's type variables");
}

/// A member annotation may be more general than the specialized method type.
/// Invisible binders are instantiated, the body is subsumed, and the exposed
/// constraint is solved at the specialized arguments.
#[test]
fn a_constrained_member_annotation_specializes_to_the_method_type() {
    let source = r#"
module Main where

class Eq a where
  eq :: a -> a -> Boolean

instance eqBoolean :: Eq Boolean where
  eq _ _ = true

class Show a where
  show :: a -> String

instance showBoolean :: Show Boolean where
  show :: forall a. Eq a => a -> String
  show _ = "ok"

data Box a = Box a

instance eqBox :: Eq a => Eq (Box a) where
  eq :: forall b. Eq b => Box b -> Box b -> Boolean
  eq _ _ = true

instance eqInt :: Eq Int where
  eq :: forall a. Eq a => a -> a -> Boolean
  eq _ _ = true
"#;
    check_program_types_lenient(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("a constrained member annotation must specialize: {errors:?}")
    });
}

#[test]
fn a_constrained_member_annotation_reports_an_unsolved_constraint() {
    let source = r#"
module Main where

class Eq a where
  eq :: a -> a -> Boolean

class Show a where
  show :: a -> String

instance showInt :: Show Int where
  show :: forall a. Eq a => a -> String
  show _ = "x"
"#;
    let errors = check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("Eq Int is not solved by specializing the member annotation");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn a_constrained_member_annotation_rejects_a_result_mismatch() {
    let source = r#"
module Main where

class Eq a where
  eq :: a -> a -> Boolean

instance eqInt :: Eq Int where
  eq :: forall a. Eq a => a -> a -> Int
  eq _ _ = 0
"#;
    let errors = check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("the specialized result must still match the method");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("TypesDoNotUnify")),
        "unexpected diagnostics: {errors:?}"
    );
}
