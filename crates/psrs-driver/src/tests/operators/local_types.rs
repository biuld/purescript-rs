use super::*;

#[test]
fn a_local_signature_with_a_type_operator_chain_is_desugared() {
    // A `where`/`let` binding signature is elaborated as a `Typed` expression.
    // Its written type must go through P4 like any other type, or the operator
    // chain survives into kind checking as `UnloweredTypeOperator`.
    let source = r#"module Main where

import Prelude

type Pair a b = { _1 :: a, _2 :: b }

infixr 5 type Pair as :+:

main :: Int
main = helper { _1: 42, _2: true }
  where
  helper :: Int :+: Boolean -> Int
  helper p = p._1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
