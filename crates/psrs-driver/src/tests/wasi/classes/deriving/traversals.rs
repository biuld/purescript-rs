use super::*;

#[test]
fn derived_traversable_executes_through_its_instances() {
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

data Box a = Box a

derive instance traversableBox :: Traversable Box

main :: Int
main = case traverse (\x -> Just (intAdd x 1)) (Box 41) of
  Just (Box x) -> x
  _ -> 0
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Data.Function.purs", function),
        ("Data.Functor.purs", functor),
        ("Control.Apply.purs", apply),
        ("Control.Applicative.purs", applicative),
        ("Data.Traversable.purs", traversable),
        ("Main.purs", main),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn derived_bitraversable_preserves_both_effects_and_sequence() {
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
    let traversable = r#"module Data.Bitraversable where

import Control.Applicative (class Applicative)

class Bitraversable t where
  bitraverse :: forall a b c d f. Applicative f => (a -> f c) -> (b -> f d) -> t a b -> f (t c d)
  bisequence :: forall a b f. Applicative f => t (f a) (f b) -> f (t a b)
"#;
    let main = r#"module Main where

import Data.Function (identity)
import Data.Bitraversable
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

data Box a b = Box a b

derive instance bitraversableBox :: Bitraversable Box

main :: Int
main = case bitraverse (\x -> Just (intAdd x 1)) (\y -> Just (intAdd y 2)) (Box 19 20) of
  Just (Box x y) -> if intEq x 20 then if intEq y 22 then case bisequence (Box (Just 20) (Just 22)) of
    Just (Box a b) -> intAdd a b
    _ -> 1
    else 2
    else 3
  _ -> 0
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Data.Function.purs", function),
        ("Data.Functor.purs", functor),
        ("Control.Apply.purs", apply),
        ("Control.Applicative.purs", applicative),
        ("Data.Bitraversable.purs", traversable),
        ("Main.purs", main),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn derived_traversable_uses_its_higher_kinded_context_dictionary() {
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

data Box f a = Box (f a)

derive instance traversableMaybe :: Traversable Maybe

derive instance traversableBox :: Traversable f => Traversable (Box f)

main :: Int
main = case traverse (\x -> Just (intAdd x 1)) (Box (Just 41)) of
  Just (Box (Just x)) -> x
  _ -> 0
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Data.Function.purs", function),
        ("Data.Functor.purs", functor),
        ("Control.Apply.purs", apply),
        ("Control.Applicative.purs", applicative),
        ("Data.Traversable.purs", traversable),
        ("Main.purs", main),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn derived_traversable_sequences_through_the_trusted_core_libraries() {
    let source = r#"module Main where
import Prelude
import Data.Maybe (Maybe(..))
import Data.Traversable (class Traversable, traverse, sequence)
import Data.Foldable (class Foldable)
data Box a = Box a
derive instance functorBox :: Functor Box
derive instance foldableBox :: Foldable Box
derive instance traversableBox :: Traversable Box
main :: Int
main = case traverse (\x -> Just (intAdd x 1)) (Box 41) of
  Just (Box x) -> if intEq x 42 then case sequence (Box (Just 42)) of
    Just (Box y) -> y
    _ -> 1
    else 2
  _ -> 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn derived_bitraversable_sequences_through_the_trusted_core_libraries() {
    let source = r#"module Main where
import Prelude
import Data.Maybe (Maybe(..))
import Data.Bifunctor (class Bifunctor)
import Data.Bifoldable (class Bifoldable)
import Data.Bitraversable (class Bitraversable, bitraverse, bisequence)
data Box a b = Box { left :: a, right :: b, inert :: Int }
derive instance bifunctorBox :: Bifunctor Box
derive instance bifoldableBox :: Bifoldable Box
derive instance bitraversableBox :: Bitraversable Box
main :: Int
main = case bitraverse (\x -> Just (intAdd x 1)) (\y -> Just (intAdd y 2)) (Box { left: 19, right: 20, inert: 7 }) of
  Just (Box r) -> if intEq r.inert 7 then case bisequence (Box { left: Just r.left, right: Just r.right, inert: 7 }) of
    Just (Box s) -> intAdd s.left s.right
    _ -> 1
    else 2
  _ -> 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
