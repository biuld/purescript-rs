//! Source-level derived instances executed through the ordinary dictionary ABI.

use super::super::super::*;

mod traversals;

#[test]
fn imported_newtype_derived_dictionary_executes_its_coercion_adapter() {
    let library = r#"module Lib (Age(..), class ToInt, toInt) where

class ToInt a where
  toInt :: a -> Int

instance toIntInt :: ToInt Int where
  toInt value = value

newtype Age = Age Int

derive newtype instance toIntAge :: ToInt Age
"#;
    let main = r#"module Main where

import Lib (Age(..), class ToInt, toInt)

main :: Int
main = toInt (Age 43)
"#;
    let Some(output) = run_program_with_wasmtime(&[("Lib.purs", library), ("Main.purs", main)])
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(43));
}

#[test]
fn imported_structural_eq_instance_executes_constructor_and_field_comparisons() {
    let library = r#"module Data.Eq where

class Eq a where
  eq :: a -> a -> Boolean

instance eqInt :: Eq Int where
  eq left right = intEq left right
"#;
    let main = r#"module Main where

import Data.Eq

data Choice a = First a | Second

derive instance eqChoice :: Eq a => Eq (Choice a)

main :: Int
main = if eq (First 42) (First 42) then if eq (First 1) (First 2) then 0 else if eq (First 1) Second then 1 else 42 else 1
"#;
    let Some(output) = run_program_with_wasmtime(&[("Data.Eq.purs", library), ("Main.purs", main)])
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn derived_ord_executes_constructor_and_lexicographic_field_order() {
    let ordering = r#"module Data.Ordering where

data Ordering = LT | EQ | GT
"#;
    let ord = r#"module Data.Ord where

import Data.Ordering

class Ord a where
  compare :: a -> a -> Ordering

"#;
    let main = r#"module Main where

import Data.Ord
import Data.Ordering

data Token = Low | High
data Pair = Pair Token Token
data Color = Red | Green | Blue

derive instance ordToken :: Ord Token
derive instance ordPair :: Ord Pair
derive instance ordColor :: Ord Color

main :: Int
main = case compare (Pair Low High) (Pair Low Low) of
  GT -> case compare Red Blue of
    LT -> 42
    _ -> 1
  _ -> 0
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Data.Ordering.purs", ordering),
        ("Data.Ord.purs", ord),
        ("Main.purs", main),
    ]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn derives_a_higher_kinded_newtype_instance_through_the_wrapped_type() {
    let source = r#"module Main where

class Head f where
  headValue :: f Int -> Int

data Maybe a = Nothing | Just a

instance headMaybe :: Head Maybe where
  headValue _ = 42

newtype First a = First (Maybe a)

derive newtype instance headFirst :: Head First

main :: Int
main = headValue (First (Just 42))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn derives_recursive_eq_through_its_instance_dictionary() {
    let library = r#"module Data.Eq where

class Eq a where
  eq :: a -> a -> Boolean

instance eqInt :: Eq Int where
  eq left right = intEq left right
"#;
    let main = r#"module Main where

import Data.Eq

data Chain a = End | Link a (Chain a)

derive instance eqChain :: Eq a => Eq (Chain a)

main :: Int
main = if eq (Link 1 (Link 2 End)) (Link 1 (Link 2 End)) then if eq (Link 1 End) (Link 2 End) then 0 else 42 else 1
"#;
    let Some(output) = run_program_with_wasmtime(&[("Data.Eq.purs", library), ("Main.purs", main)])
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn derives_nested_functor_mapping_through_an_imported_dictionary() {
    let library = r#"module Data.Functor where

class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b

data Option a = None | Some a

instance functorOption :: Functor Option where
  map f value = case value of
    None -> None
    Some item -> Some (f item)
"#;
    let main = r#"module Main where

import Data.Functor

type Wrapped a = Option a

data Box a = Box (Wrapped a)

derive instance functorBox :: Functor Box

main :: Int
main = case map (\value -> intAdd value 1) (Box (Some 41)) of
  Box (Some value) -> value
  _ -> 0
"#;
    let Some(output) =
        run_program_with_wasmtime(&[("Data.Functor.purs", library), ("Main.purs", main)])
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn derived_foldable_executes_through_its_instances() {
    let semigroup = r#"module Data.Semigroup where

class Semigroup a where
  append :: a -> a -> a
"#;
    let monoid = r#"module Data.Monoid where

import Data.Semigroup

class Semigroup a <= Monoid a where
  mempty :: a
"#;
    let foldable = r#"module Data.Foldable where

import Data.Monoid

class Foldable t where
  foldr :: forall a b. (a -> b -> b) -> b -> t a -> b
  foldl :: forall a b. (b -> a -> b) -> b -> t a -> b
  foldMap :: forall a m. Monoid m => (a -> m) -> t a -> m
"#;
    let main = r#"module Main where

import Data.Foldable
import Data.Monoid
import Data.Semigroup

instance semigroupInt :: Semigroup Int where
  append left right = intAdd left right

instance monoidInt :: Monoid Int where
  mempty = 0

data Box a = Box a

derive instance foldableBox :: Foldable Box

main :: Int
main = intAdd (foldMap (\x -> x) (Box 40)) (foldr (\x acc -> intAdd x acc) 0 (Box 2))
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Data.Semigroup.purs", semigroup),
        ("Data.Monoid.purs", monoid),
        ("Data.Foldable.purs", foldable),
        ("Main.purs", main),
    ]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn derives_bifunctor_mapping_for_both_type_parameters() {
    let bifunctor = r#"module Data.Bifunctor where

class Bifunctor f where
  bimap :: forall a b c d. (a -> b) -> (c -> d) -> f a c -> f b d
"#;
    let main = r#"module Main where

import Data.Bifunctor

data Pair a b = Pair a b | Left a | Right b

derive instance bifunctorPair :: Bifunctor Pair

main :: Int
main = case bimap (\value -> intAdd value 1) (\value -> intAdd value 1) (Pair 40 0) of
  Pair left right -> intAdd left right
  _ -> 0
"#;
    let Some(output) =
        run_program_with_wasmtime(&[("Data.Bifunctor.purs", bifunctor), ("Main.purs", main)])
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

mod adapters;
mod generic;

mod higher_kinded;
mod records;

mod folds;
