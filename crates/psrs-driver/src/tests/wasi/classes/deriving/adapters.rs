use super::*;

#[test]
fn polymorphic_newtype_deriving_executes_its_adapter() {
    let source = r#"module Main where

class ToInt a where
  toInt :: forall b. a -> b -> Int

instance toIntInt :: ToInt Int where
  toInt value _ = value

newtype Age = Age Int

derive newtype instance toIntAge :: ToInt Age

main :: Int
main = if intEq (toInt (Age 42) true) 42 then toInt (Age 42) [1, 2] else 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn function_contravariant_deriving_executes_its_adapter() {
    let profunctor = r#"module Data.Profunctor where

class Profunctor p where
  dimap :: forall a b c d. (b -> a) -> (c -> d) -> p a c -> p b d
  lcmap :: forall a b c. (b -> a) -> p a c -> p b c
  rmap :: forall a b c. (b -> c) -> p a b -> p a c

instance profunctorFunction :: Profunctor (->) where
  dimap f g h = \x -> g (h (f x))
  lcmap f h = \x -> h (f x)
  rmap f h = \x -> f (h x)
"#;
    let contravariant = r#"module Data.Functor.Contravariant where

class Contravariant f where
  cmap :: forall a b. (b -> a) -> f a -> f b
"#;
    let main = r#"module Main where

import Data.Functor.Contravariant
import Data.Profunctor (class Profunctor)

data Predicate a = Predicate (a -> Boolean)

derive instance contravariantPredicate :: Contravariant Predicate

main :: Int
main = case cmap (\x -> intAdd x 1) (Predicate (\x -> intEq x 42)) of
  Predicate f -> if f 41 then if f 40 then 0 else 42 else 1
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Data.Profunctor.purs", profunctor),
        ("Data.Functor.Contravariant.purs", contravariant),
        ("Main.purs", main),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn profunctor_deriving_maps_record_function_input_and_result() {
    let profunctor = r#"module Data.Profunctor where

class Profunctor p where
  dimap :: forall a b c d. (b -> a) -> (c -> d) -> p a c -> p b d

instance profunctorFunction :: Profunctor (->) where
  dimap f g h = \x -> g (h (f x))
"#;
    let contravariant = r#"module Data.Functor.Contravariant where

class Contravariant f where
  cmap :: forall a b. (b -> a) -> f a -> f b
"#;
    let main = r#"module Main where

import Data.Functor.Contravariant
import Data.Profunctor

data Predicate a b = Predicate { run :: a -> b, inert :: Int }

derive instance profunctorPredicate :: Profunctor Predicate

main :: Int
main = case dimap (\x -> intAdd x 1) (\y -> intAdd y 2) (Predicate { run: \x -> x, inert: 7 }) of
  Predicate r -> if intEq r.inert 7 then r.run 39 else 0
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Data.Profunctor.purs", profunctor),
        ("Data.Functor.Contravariant.purs", contravariant),
        ("Main.purs", main),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn contravariant_deriving_maps_record_predicates_and_retains_inert_fields() {
    let profunctor = r#"module Data.Profunctor where

class Profunctor p where
  dimap :: forall a b c d. (b -> a) -> (c -> d) -> p a c -> p b d
  lcmap :: forall a b c. (b -> a) -> p a c -> p b c
  rmap :: forall a b c. (b -> c) -> p a b -> p a c

instance profunctorFunction :: Profunctor (->) where
  dimap f g h = \x -> g (h (f x))
  lcmap f h = \x -> h (f x)
  rmap f h = \x -> f (h x)
"#;
    let contravariant = r#"module Data.Functor.Contravariant where

class Contravariant f where
  cmap :: forall a b. (b -> a) -> f a -> f b
"#;
    let main = r#"module Main where

import Data.Functor.Contravariant
import Data.Profunctor (class Profunctor)

data Predicate a = Predicate { run :: a -> Boolean, inert :: Int }

derive instance contravariantPredicate :: Contravariant Predicate

main :: Int
main = case cmap (\x -> intAdd x 1) (Predicate { run: \x -> intEq x 42, inert: 7 }) of
  Predicate r -> if intEq r.inert 7 then if r.run 41 then if r.run 40 then 0 else 42 else 1 else 2
"#;
    let Some(output) = run_program_with_wasmtime(&[
        ("Data.Profunctor.purs", profunctor),
        ("Data.Functor.Contravariant.purs", contravariant),
        ("Main.purs", main),
    ]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}
