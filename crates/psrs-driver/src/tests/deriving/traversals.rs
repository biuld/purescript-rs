#[test]
fn derives_bitraversable_for_a_two_parameter_type() {
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
    let bitraversable = r#"module Data.Bitraversable where

import Control.Applicative (class Applicative)

class Bitraversable t where
  bitraverse :: forall f a b c d. Applicative f => (a -> f c) -> (b -> f d) -> t a b -> f (t c d)
  bisequence :: forall f a b. Applicative f => t (f a) (f b) -> f (t a b)
"#;
    let main = r#"module Main where

import Data.Function (identity)
import Data.Bitraversable

data P a b = P a b

derive instance bitraversableP :: Bitraversable P

main :: Int
main = 0
"#;
    crate::check_program(&[
        ("Data.Function.purs", function),
        ("Data.Functor.purs", functor),
        ("Control.Apply.purs", apply),
        ("Control.Applicative.purs", applicative),
        ("Data.Bitraversable.purs", bitraversable),
        ("Main.purs", main),
    ])
    .unwrap_or_else(|errors| panic!("Bitraversable deriving should type check: {errors:?}"));
}

#[test]
fn derives_traversable_for_single_field_constructors() {
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

data Box a = Box a

derive instance traversableBox :: Traversable Box

main :: Int
main = 0
"#;
    crate::check_program(&[
        ("Data.Function.purs", function),
        ("Data.Functor.purs", functor),
        ("Control.Apply.purs", apply),
        ("Control.Applicative.purs", applicative),
        ("Data.Traversable.purs", traversable),
        ("Main.purs", main),
    ])
    .unwrap_or_else(|errors| panic!("Traversable deriving should type check: {errors:?}"));
}

#[test]
fn derives_foldable_for_single_field_constructors() {
    let monoid = r#"module Data.Monoid where

class Monoid a where
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

data Box a = Box a

derive instance foldableBox :: Foldable Box

main :: Int
main = 0
"#;
    crate::check_program(&[
        ("Data.Monoid.purs", monoid),
        ("Data.Foldable.purs", foldable),
        ("Main.purs", main),
    ])
    .unwrap_or_else(|errors| panic!("Foldable deriving should type check: {errors:?}"));
}

#[test]
fn derives_bifoldable_for_a_two_parameter_type() {
    let semigroup = r#"module Data.Semigroup where

class Semigroup a where
  append :: a -> a -> a
"#;
    let monoid = r#"module Data.Monoid where

import Data.Semigroup

class Semigroup a <= Monoid a where
  mempty :: a
"#;
    let bifoldable = r#"module Data.Bifoldable where

import Data.Monoid

class Bifoldable p where
  bifoldr :: forall a b c. (a -> c -> c) -> (b -> c -> c) -> c -> p a b -> c
  bifoldl :: forall a b c. (c -> a -> c) -> (c -> b -> c) -> c -> p a b -> c
  bifoldMap :: forall m a b. Monoid m => (a -> m) -> (b -> m) -> p a b -> m
"#;
    let main = r#"module Main where

import Data.Bifoldable

data P a b = P a b

derive instance bifoldableP :: Bifoldable P

main :: Int
main = 0
"#;
    crate::check_program(&[
        ("Data.Semigroup.purs", semigroup),
        ("Data.Monoid.purs", monoid),
        ("Data.Bifoldable.purs", bifoldable),
        ("Main.purs", main),
    ])
    .unwrap_or_else(|errors| panic!("Bifoldable deriving should type check: {errors:?}"));
}
