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

#[test]
fn library_tuple_equality_ordering_and_show_typecheck() {
    let main = r#"
module Main where

import Data.Eq (eq)
import Data.Ord (compare, Ordering)
import Data.Show (show)
import Data.Tuple (Tuple(..))

equalPair :: Boolean
equalPair = eq (Tuple 1 "雪") (Tuple 1 "雪")

orderedPair :: Ordering
orderedPair = compare (Tuple 1 "z") (Tuple 2 "a")

shownPair :: String
shownPair = show (Tuple 1 "雪")

main :: Int
main = 0
"#;
    crate::check_program_types_lenient_with_prelude(&[("Main.purs", main)])
        .unwrap_or_else(|errors| panic!("tuple instances should type check: {errors:?}"));
}

#[test]
fn library_tuple_instances_compare_fields_and_render_utf8() {
    let main = r#"
module Main where

import Data.Eq (eq)
import Data.Ord (compare, Ordering(..))
import Data.Show (show)
import Data.Tuple (Tuple(..))

main :: Int
main =
  if eq (Tuple 1 "雪") (Tuple 1 "雪") then
    if eq (Tuple 1 "雪") (Tuple 1 "a") then 1
    else case compare (Tuple 1 99) (Tuple 2 0) of
      LT -> case compare (Tuple 1 3) (Tuple 1 2) of
        GT -> case compare (Tuple 1 2) (Tuple 1 2) of
          EQ -> if eq (show (Tuple 1 "雪")) "(Tuple 1 \"雪\")" then 0 else 4
          _ -> 3
        _ -> 2
      _ -> 1
  else 1
"#;
    let Some(output) = run_with_wasmtime(main) else {
        eprintln!("skipping execution: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}
