mod diagnostics;
mod traversals;

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
  eq left right = intEq left right
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
fn derives_eq1_and_ord1_by_delegating_to_the_monomorphic_method() {
    let eq = r#"module Data.Eq where

class Eq a where
  eq :: a -> a -> Boolean

class Eq1 f where
  eq1 :: forall a. Eq a => f a -> f a -> Boolean
"#;
    let ordering = r#"module Data.Ordering where

data Ordering = LT | EQ | GT
"#;
    let ord = r#"module Data.Ord where

import Data.Ordering

class Ord a where
  compare :: a -> a -> Ordering

class Ord1 f where
  compare1 :: forall a. Ord a => f a -> f a -> Ordering
"#;
    let main = r#"module Main where

import Data.Eq
import Data.Ord
import Data.Ordering

data Maybe a = Nothing | Just a

derive instance eqMaybe :: Eq a => Eq (Maybe a)
derive instance eq1Maybe :: Eq1 Maybe

derive instance ordMaybe :: Ord a => Ord (Maybe a)
derive instance ord1Maybe :: Ord1 Maybe

main :: Int
main = 0
"#;
    crate::check_program(&[
        ("Data.Eq.purs", eq),
        ("Data.Ordering.purs", ordering),
        ("Data.Ord.purs", ord),
        ("Main.purs", main),
    ])
    .unwrap_or_else(|errors| panic!("Eq1/Ord1 deriving should type check: {errors:?}"));
}

#[test]
fn derives_generic_representation_for_a_data_type() {
    let generic_rep = r#"module Data.Generic.Rep where

data NoConstructors

data NoArguments = NoArguments

newtype Argument a = Argument a

data Product a b = Product a b

data Sum a b = Inl a | Inr b

newtype Constructor (name :: Symbol) a = Constructor a

class Generic t rep | t -> rep where
  from :: t -> rep
  to :: rep -> t
"#;
    let main = r#"module Main where

import Data.Generic.Rep

data Maybe a = Nothing | Just a

derive instance genericMaybe :: Generic (Maybe a) _

main :: Int
main = 0
"#;
    crate::check_program(&[("Data.Generic.Rep.purs", generic_rep), ("Main.purs", main)])
        .unwrap_or_else(|errors| panic!("Generic deriving should type check: {errors:?}"));
}

#[test]
fn derives_newtype_class_for_a_newtype_with_a_wildcard() {
    let newtype_module = r#"module Data.Newtype where

class Newtype t a
"#;
    let main = r#"module Main where

import Data.Newtype (class Newtype)

newtype Age = Age Int

derive instance newtypeAge :: Newtype Age _

main :: Int
main = 0
"#;
    crate::check_program(&[("Data.Newtype.purs", newtype_module), ("Main.purs", main)])
        .unwrap_or_else(|errors| panic!("Newtype wildcard deriving should type check: {errors:?}"));
}

#[test]
fn derives_profunctor_through_a_contravariant_field() {
    let profunctor = r#"module Data.Profunctor where

class Profunctor p where
  dimap :: forall a b c d. (a -> b) -> (c -> d) -> p b c -> p a d
"#;
    let contravariant = r#"module Data.Functor.Contravariant where

class Contravariant f where
  cmap :: forall a b. (b -> a) -> f a -> f b
"#;
    let main = r#"module Main where

import Data.Profunctor (class Profunctor)
import Data.Functor.Contravariant (class Contravariant)

newtype Predicate a = Predicate (a -> Boolean)

instance contravariantPredicate :: Contravariant Predicate where
  cmap f (Predicate g) = Predicate (\x -> g (f x))

data P a b = P (Predicate a) b

derive instance profunctorP :: Profunctor P

main :: Int
main = 0
"#;
    crate::check_program(&[
        ("Data.Profunctor.purs", profunctor),
        ("Data.Functor.Contravariant.purs", contravariant),
        ("Main.purs", main),
    ])
    .unwrap_or_else(|errors| panic!("Profunctor deriving should type check: {errors:?}"));
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
