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
