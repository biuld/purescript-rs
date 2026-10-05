//! Library tuples and native tuple syntax keep their distinct representations.

use super::*;

#[test]
fn library_tuple_constructor_and_helpers_execute() {
    let main = r#"
module Main where

import Data.Tuple (Tuple(..), curry, fst, snd, swap, uncurry)

pair :: Tuple Int Int
pair = Tuple 40 2

main = if intEq (uncurry (\left right -> intAdd left right) pair) 42
  then if intEq (fst (swap pair)) 2
    then if intEq (snd (swap pair)) 40
      then if intEq (curry (\tuple -> intAdd (fst tuple) (snd tuple)) 20 22) 42 then 0 else 1
      else 1
    else 1
  else 1
"#;
    let Some(output) = run_with_wasmtime(main) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn native_tuple_syntax_still_has_the_closed_record_representation() {
    let main = r#"
module Main where

import Data.Tuple (Tuple)

type Pair = { _1 :: Int, _2 :: Int }

fromSyntax :: Pair
fromSyntax = (40, 2)

fromRecord :: Pair
fromRecord = { _1: 40, _2: 2 }

main = 0
"#;
    let Some(output) = run_with_wasmtime(main) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}
