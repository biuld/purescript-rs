#[test]
fn derives_newtype_methods_from_the_wrapped_instance() {
    let sources = [(
        "Main.purs",
        r#"module Main where

class ToInt a where
  toInt :: a -> Int

instance toIntInt :: ToInt Int where
  toInt value = value

newtype Age = Age Int

derive newtype instance toIntAge :: ToInt Age

main :: Int
main = toInt (Age 42)
"#,
    )];
    crate::check_program(&sources)
        .unwrap_or_else(|errors| panic!("derive newtype should type check: {errors:?}"));
}

#[test]
fn contravariant_deriving_uses_the_profunctor_dictionary_for_function_inputs() {
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
main = 0
"#;
    crate::check_program(&[
        ("Data.Profunctor.purs", profunctor),
        ("Data.Functor.Contravariant.purs", contravariant),
        ("Main.purs", main),
    ])
    .unwrap_or_else(|errors| panic!("Contravariant deriving should type check: {errors:?}"));
}

#[test]
fn newtype_deriving_accepts_a_polymorphic_class_method() {
    let source = r#"module Main where

class ToInt a where
  toInt :: forall b. a -> b -> Int

instance toIntInt :: ToInt Int where
  toInt value _ = value

newtype Age = Age Int

derive newtype instance toIntAge :: ToInt Age

main :: Int
main = toInt (Age 42) true
"#;
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("newtype derivation should preserve method polymorphism: {errors:?}")
    });
}

#[test]
fn rejects_newtype_deriving_for_a_data_declaration() {
    let sources = [(
        "Main.purs",
        r#"module Main where

class ToInt a where
  toInt :: a -> Int

data Box = Box Int

derive newtype instance toIntBox :: ToInt Box

main :: Int
main = 0
"#,
    )];
    let errors =
        crate::check_program(&sources).expect_err("data is not eligible for newtype deriving");
    assert!(
        errors.iter().any(|error| {
            error
                .diagnostic
                .message
                .contains("locally declared newtype constructor")
        }),
        "unexpected diagnostics: {errors:?}"
    );
}

#[test]
fn derives_eq_for_an_algebraic_data_type() {
    let eq = r#"module Data.Eq where

class Eq a where
  eq :: a -> a -> Boolean

instance eqInt :: Eq Int where
  eq left right = left == right
"#;
    let main = r#"module Main where

import Data.Eq

data Choice a = First a | Second

derive instance eqChoice :: Eq a => Eq (Choice a)

main :: Int
main = 42
"#;
    crate::check_program(&[("Data.Eq.purs", eq), ("Main.purs", main)])
        .unwrap_or_else(|errors| panic!("derived Eq should type check: {errors:?}"));
}

#[test]
fn derives_lexicographic_ord_for_constructor_and_field_order() {
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

derive instance ordToken :: Ord Token
derive instance ordPair :: Ord Pair

main :: Int
main = case compare (Pair Low High) (Pair Low Low) of
  GT -> 42
  _ -> 0
"#;
    crate::check_program(&[
        ("Data.Ordering.purs", ordering),
        ("Data.Ord.purs", ord),
        ("Main.purs", main),
    ])
    .unwrap_or_else(|errors| panic!("derived Ord should type check: {errors:?}"));
}

#[test]
fn derives_newtype_for_a_partially_applied_type_constructor() {
    let source = r#"module Main where

class Head f where
  headValue :: f Int -> Int

data Maybe a = Nothing | Just a

instance headMaybe :: Head Maybe where
  headValue (Just value) = value
  headValue Nothing = 0

newtype First a = First (Maybe a)

derive newtype instance headFirst :: Head First

main :: Int
main = headValue (First (Just 42))
"#;
    crate::check_program(&[("Main.purs", source)]).unwrap_or_else(|errors| {
        panic!("partially applied newtype derivation should type check: {errors:?}")
    });
}
