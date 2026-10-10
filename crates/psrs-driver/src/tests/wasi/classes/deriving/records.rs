use super::*;

#[test]
fn derived_functor_maps_record_fields_and_retains_inert_fields() {
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

data Box a = Box { nested :: Wrapped a, scalar :: a, flag :: Boolean }

derive instance functorBox :: Functor Box

main :: Int
main = case map (\value -> intAdd value 1) (Box { nested: Some 20, scalar: 20, flag: true }) of
  Box r -> case r.nested of
    Some x -> if r.flag then intAdd x r.scalar else 0
    _ -> 0
"#;
    let Some(output) =
        run_program_with_wasmtime(&[("Data.Functor.purs", library), ("Main.purs", main)])
    else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn derived_foldable_preserves_record_label_order_in_both_fold_directions() {
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

data Box a = Box { first :: a, last :: a, flag :: Boolean }

derive instance foldableBox :: Foldable Box

value = Box { first: 1, last: 2, flag: true }
main :: Int
main = if intEq (foldMap (\x -> x) value) 3 then
  if intEq (foldr (\x acc -> intAdd x (intMul 10 acc)) 0 value) 21 then
    if intEq (foldl (\acc x -> intAdd (intMul 10 acc) x) 0 value) 12 then 42 else 1
  else 2
  else 3
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
    assert_eq!(
        output.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn derived_traversable_maps_record_effects_and_preserves_inert_fields() {
    let function = r#"module Data.Function where

identity :: forall a. a -> a
identity x = x
"#;
    let functor = r#"module Data.Functor where

class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b
"#;
    let apply = r#"module Control.Apply where

import Data.Functor

class Functor f <= Apply f where
  apply :: forall a b. f (a -> b) -> f a -> f b
"#;
    let applicative = r#"module Control.Applicative where

import Control.Apply

class Apply f <= Applicative f where
  pure :: forall a. a -> f a
"#;
    let traversable = r#"module Data.Traversable where

import Control.Applicative (class Applicative)

class Traversable t where
  traverse :: forall a b f. Applicative f => (a -> f b) -> t a -> f (t b)
  sequence :: forall a f. Applicative f => t (f a) -> f (t a)
"#;
    let main = r#"module Main where

import Data.Function (identity)
import Data.Traversable
import Data.Functor
import Control.Apply
import Control.Applicative

data Maybe a = Nothing | Just a
instance functorMaybe :: Functor Maybe where
  map f value = case value of
    Nothing -> Nothing
    Just x -> Just (f x)
instance applyMaybe :: Apply Maybe where
  apply f value = case f of
    Nothing -> Nothing
    Just g -> map g value
instance applicativeMaybe :: Applicative Maybe where
  pure = Just

data Box a = Empty Int | Box { first :: a, last :: a, inert :: Int }

derive instance traversableBox :: Traversable Box

main :: Int
main = case traverse (\x -> Just (intAdd x 1)) (Box { first: 20, last: 20, inert: 7 }) of
  Just (Box r) -> if intEq r.inert 7 then case sequence (Empty 9 :: Box (Maybe Int)) of
    Just (Empty n) -> if intEq n 9 then intAdd r.first r.last else 1
    _ -> 2
    else 3
  _ -> 0
"#;
    let sources = [
        ("Data.Function.purs", function),
        ("Data.Functor.purs", functor),
        ("Control.Apply.purs", apply),
        ("Control.Applicative.purs", applicative),
        ("Data.Traversable.purs", traversable),
        ("Main.purs", main),
    ];
    let Some(output) = run_program_with_wasmtime(&sources) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bifunctor_deriving_maps_both_record_parameters_and_retains_inert_fields() {
    let bifunctor = r#"module Data.Bifunctor where
class Bifunctor f where
  bimap :: forall a b c d. (a -> b) -> (c -> d) -> f a c -> f b d
"#;
    let main = r#"module Main where
import Data.Bifunctor
data Box a b = Box { left :: a, right :: b, inert :: Int }
derive instance bifunctorBox :: Bifunctor Box
main :: Int
main = case bimap (\x -> intAdd x 1) (\y -> intAdd y 2) (Box { left: 19, right: 20, inert: 7 }) of
  Box r -> if intEq r.inert 7 then intAdd r.left r.right else 0
"#;
    let Some(output) =
        run_program_with_wasmtime(&[("Data.Bifunctor.purs", bifunctor), ("Main.purs", main)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
