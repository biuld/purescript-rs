//! The library's `Monoid` and `Foldable` classes.
//!
//! `foldMap` and `fold` are constrained by `Monoid`, so the two classes land
//! together. These tests run the instances, not a second traversal: `Array`
//! walks the compiler's index primitives, and `Maybe` and `Either` fold by
//! cases.

use super::*;

#[test]
fn monoid_identities_and_folds_run_under_wasmtime() {
    let source = r#"
module Main where

import Prelude
import Effect.Console (log)
import Data.Monoid (mempty)
import Data.Foldable (class Foldable, foldl, foldr, foldMap)
import Data.Maybe (Maybe(..))
import Data.Either (Either(..))

data One a = One a

instance foldableOne :: Foldable One where
  foldr f z (One x) = f x z
  foldl f z (One x) = f z x
  foldMap f (One x) = f x

checks :: Effect Unit
checks = do
  log (foldr (\x acc -> x <> acc) ":" ["a", "b"])
  log (foldl (\acc x -> acc <> x) ":" ["a", "b"])
  log (foldMap (\x -> x <> "!") ["a", "b"])
  log (if arrayLength (mempty :: Array Int) == 0 then "empty-array" else "empty-array-wrong")
  log (if (mempty :: Unit) == unit then "empty-unit" else "empty-unit-wrong")
  log mempty
  log (foldr (\x acc -> x <> acc) "z" (Right "r" :: Either Int String))
  log (foldr (\x acc -> x <> acc) "z" (Left 1 :: Either Int String))
  log (foldl (\acc x -> acc <> x) "z" (Just "m"))
  log (foldMap (\x -> x) (Nothing :: Maybe String))
  log (foldMap (\x -> x) (One "one"))
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
        "ab:\n:ab\na!b!\nempty-array\nempty-unit\n\nrz\nz\nzm\n\none\n".as_bytes(),
        "monoid identities and folds must match the class instances: {output:?}"
    );
}
