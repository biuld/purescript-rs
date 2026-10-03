//! The library's `Data.Tuple` surface.
//!
//! `Tuple a b` is the closed record `{ _1 :: a, _2 :: b }` that FE-06 already
//! lowers a tuple to, not an algebraic product. These tests check that the
//! synonym, the tuple syntax, and a record literal are one value, and that the
//! projection functions execute under Wasmtime.

use super::*;

#[test]
fn a_tuple_is_the_closed_record_and_executes_under_wasmtime() {
    let source = r#"
module Main where

import Prelude
import Data.Tuple (Tuple, curry, fst, snd, swap, uncurry)

fromSyntax :: Tuple Int Int
fromSyntax = (40, 2)

fromRecord :: Tuple Int Int
fromRecord = { _1: 40, _2: 2 }

passed :: Boolean -> Int
passed flag = if flag then 1 else 0

main :: Int
main =
  passed (uncurry (\a b -> a + b) fromSyntax == 42)
    + passed (uncurry (\a b -> a + b) fromRecord == 42)
    + passed (fst fromSyntax == 40)
    + passed (snd fromRecord == 2)
    + passed (fst (swap fromSyntax) == 2)
    + passed (snd (swap fromRecord) == 40)
    + passed (curry (\pair -> fst pair + snd pair) 20 22 == 42)
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(7),
        "tuple syntax and the record form must be the same value: {output:?}"
    );
}
