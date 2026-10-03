//! The library's `Semigroup` class and its `<>` operator.
//!
//! `Semigroup` is the first class the library itself declares, so these tests
//! check more than concatenation: they check that a library-declared class, its
//! method, and an operator alias reach a user module through `Prelude`'s
//! selective re-export, and that instance resolution picks different instances
//! for the same operator.

use super::*;

#[test]
fn array_append_is_a_core_expression() {
    // `arrayAppend` is a compiler intrinsic with no source spelling for the
    // array it creates. Core must name that expression rather than replacing it
    // with an unlowered global reference, which no backend could emit.
    let source = r#"
module Main where

import Prelude

main :: Int
main = arrayLength ([1, 2] <> [3])
"#;
    let core =
        lower_source_to_core("Main.purs", source).expect("array append should lower to Core");
    let dump = format!("{core:#?}");
    assert!(
        dump.contains("ArrayAppend"),
        "Core must carry the append as its own expression: {dump}"
    );
}

#[test]
fn the_semigroup_operator_concatenates_strings_and_arrays() {
    // The operator resolves through `Prelude`, which re-exports it from the
    // library's `Data.Semigroup`. Strings and arrays use the same operator but
    // different instances, and the non-ASCII case checks that the string
    // instance keeps the one UTF-8 representation.
    let source = r#"
module Main where

import Prelude
import Effect.Console (log)

label :: String
label = "a" <> "b" <> "c"

nonAscii :: String
nonAscii = "é" <> "😀"

numbers :: Array Int
numbers = [1, 2] <> [3, 4] <> [5]

checks :: Effect Unit
checks = do
  log label
  log nonAscii
  log (if arrayLength numbers == 5 then "length-ok" else "length-wrong")
  log (if arrayIndex numbers 4 == 5 then "element-ok" else "element-wrong")
  pure unit

main = let ignored = runEffect checks in 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        output.stdout,
        "abc\né😀\nlength-ok\nelement-ok\n".as_bytes(),
        "string and array append must reach the same operator: {output:?}"
    );
}

#[test]
fn the_semigroup_operator_is_right_associative() {
    // `append` is declared `infixr 5`, so `a <> b <> c` groups as
    // `a <> (b <> c)`. `Diff`'s `append` is subtraction, which is not
    // associative, so the grouping is observable: right association gives
    // `10 - (3 - 2) = 9` and left association `(10 - 3) - 2 = 5`.
    let source = r#"
module Main where

import Prelude

newtype Diff = Diff Int

unwrap :: Diff -> Int
unwrap (Diff value) = value

instance semigroupDiff :: Semigroup Diff where
  append (Diff a) (Diff b) = Diff (a - b)

main :: Int
main = unwrap (Diff 10 <> Diff 3 <> Diff 2)
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(9),
        "`<>` must be right-associative: {output:?}"
    );
}

#[test]
fn a_type_without_an_instance_is_rejected() {
    // The operator is class-constrained, so a type with no `Semigroup`
    // instance cannot use it. A permissive lowering that ignored the
    // constraint would accept this program.
    let source = r#"
module Main where

import Prelude

data Box = Box

main :: Int
main = arrayLength (Box <> Box)
"#;
    let errors = check_program_types_lenient_with_prelude(&[("Main.purs", source)])
        .expect_err("Box is not a Semigroup");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
        "expected NoInstanceFound, got {errors:?}"
    );
}
