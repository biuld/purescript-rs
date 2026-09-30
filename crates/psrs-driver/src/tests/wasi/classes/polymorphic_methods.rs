//! Tests scoped method quantifiers that shadow class parameters.

use super::super::super::*;

#[test]
fn a_shadowing_method_forall_instantiates_at_multiple_types() {
    let source = r#"module Main where

class C b a | b -> a where
  ident :: forall a. b -> a -> a

instance cIntBoolean :: C Int Boolean where
  ident _ value = value

main :: Int
main = if ident 42 true then ident 42 7 else 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(7));
}
