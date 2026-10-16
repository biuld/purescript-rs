use super::*;

#[test]
fn derived_bifoldable_preserves_left_right_mapping_and_fold_order() {
    let semigroup = r#"module Data.Semigroup where

class Semigroup a where
  append :: a -> a -> a
"#;
    let monoid = r#"module Data.Monoid where

import Data.Semigroup

class Semigroup a <= Monoid a where
  mempty :: a
"#;
    let foldable = r#"module Data.Bifoldable where

import Data.Monoid

class Bifoldable t where
  bifoldr :: forall a b c. (a -> c -> c) -> (b -> c -> c) -> c -> t a b -> c
  bifoldl :: forall a b c. (c -> a -> c) -> (c -> b -> c) -> c -> t a b -> c
  bifoldMap :: forall a b m. Monoid m => (a -> m) -> (b -> m) -> t a b -> m
"#;
    let main = r#"module Main where

import Data.Bifoldable
import Data.Monoid
import Data.Semigroup

instance semigroupInt :: Semigroup Int where
  append left right = intAdd left right

instance monoidInt :: Monoid Int where
  mempty = 0

data Box a b = Empty | Box { left :: a, right :: b, inert :: Int }

derive instance bifoldableBox :: Bifoldable Box

main :: Int
main = if intEq (bifoldMap (\x -> x) (\y -> intAdd y 1) (Box { left: 20, right: 21, inert: 7 })) 42
  then if intEq (bifoldr (\x acc -> intAdd x (intMul 10 acc)) (\x acc -> intAdd x (intMul 10 acc)) 0 (Box { left: 1, right: 2, inert: 7 })) 21
    then if intEq (bifoldl (\acc x -> intAdd (intMul 10 acc) x) (\acc x -> intAdd (intMul 10 acc) x) 0 (Box { left: 1, right: 2, inert: 7 })) 12
      then if intEq (bifoldMap (\x -> x) (\y -> y) (Empty :: Box Int Int)) 0 then 42 else 1
      else 2
    else 3
  else 4
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Data.Semigroup.purs", semigroup),
        ("Data.Monoid.purs", monoid),
        ("Data.Bifoldable.purs", foldable),
        ("Main.purs", main),
    ]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}
