use super::*;

#[test]
fn differential_deriving_rules_against_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }

    let eq_module = r#"module Data.Eq where

class Eq a where
  eq :: a -> a -> Boolean
"#;
    let structural_eq = r#"module Main where

import Data.Eq

data Choice a = First a | Second

derive instance eqChoice :: Eq a => Eq (Choice a)

main :: Int
main = 0
"#;
    let newtype = r#"module Main where

class ToInt a where
  toInt :: a -> Int

instance toIntInt :: ToInt Int where
  toInt value = value

newtype Age = Age Int

derive newtype instance toIntAge :: ToInt Age

main :: Int
main = toInt (Age 42)
"#;
    let higher_kinded_newtype = r#"module Main where

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
    let ordering = r#"module Data.Ordering where

data Ordering = LT | EQ | GT
"#;
    let ord = r#"module Data.Ord where

import Data.Ordering

class Ord a where
  compare :: a -> a -> Ordering

"#;
    let structural_ord = r#"module Main where

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
    let functor_module = r#"module Data.Functor where

class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b

data Option a = None | Some a

instance functorOption :: Functor Option where
  map f value = case value of
    None -> None
    Some item -> Some (f item)

instance functorFunction :: Functor ((->) r) where
  map f g = \value -> f (g value)

instance functorArray :: Functor Array where
  map _ _ = []
"#;
    let nested_functor = r#"module Main where

import Data.Functor

type Wrapped a = Option a

data Box a = Box (Wrapped a)

derive instance functorBox :: Functor Box

main :: Int
main = 0
"#;
    let functor_function_result = r#"module Main where

import Data.Functor

data Box a = Box (Int -> Option a)

derive instance functorBox :: Functor Box

main :: Int
main = 0
"#;
    let functor_nested_function_array = r#"module Main where

import Data.Functor

data Box a = Box (Array (Int -> Array a))

derive instance functorBox :: Functor Box

main :: Int
main = 0
"#;
    let contravariant_functor = r#"module Main where

import Data.Functor (class Functor)

data Contra a = Contra (a -> Int)

derive instance functorContra :: Functor Contra

main :: Int
main = 0
"#;
    let bifunctor_module = r#"module Data.Bifunctor where

class Bifunctor f where
  bimap :: forall a b c d. (a -> b) -> (c -> d) -> f a c -> f b d
"#;
    let bifunctor_pair = r#"module Main where

import Data.Bifunctor

data Pair a b = Pair a b | Left a | Right b

derive instance bifunctorPair :: Bifunctor Pair

main :: Int
main = 0
"#;
    let invalid_bifunctor = r#"module Main where

import Data.Bifunctor (class Bifunctor)

data Bad a b = Bad (a -> b)

derive instance bifunctorBad :: Bifunctor Bad

main :: Int
main = 0
"#;
    let contravariant_module = r#"module Data.Functor.Contravariant where

class Contravariant f where
  cmap :: forall a b. (b -> a) -> f a -> f b
"#;
    let profunctor_module = r#"module Data.Profunctor where

class Profunctor p where
  dimap :: forall a b c d. (b -> a) -> (c -> d) -> p a c -> p b d
  lcmap :: forall a b c. (b -> a) -> p a c -> p b c
  rmap :: forall a b c. (b -> c) -> p a b -> p a c

instance profunctorFunction :: Profunctor (->) where
  dimap f g h = \x -> g (h (f x))
  lcmap f h = \x -> h (f x)
  rmap f h = \x -> f (h x)
"#;
    let contravariant_function = r#"module Main where

import Data.Functor.Contravariant
import Data.Profunctor (class Profunctor)

data Predicate a = Predicate (a -> Boolean)

derive instance contravariantPredicate :: Contravariant Predicate

main :: Int
main = 0
"#;
    let invalid_contravariant = r#"module Main where

import Data.Functor.Contravariant (class Contravariant)

data Covariant a = Covariant a

derive instance contravariantCovariant :: Contravariant Covariant

main :: Int
main = 0
"#;
    let custom_prelude_eq = r#"module Prelude where

class Eq a where
  eq :: a -> a -> Boolean
"#;
    let custom_prelude_eq_main = r#"module Main where

import Prelude

data Token = Token

derive instance eqToken :: Eq Token

main :: Int
main = 0
"#;
    let user_eq = r#"module Main where

class Eq a where
  eq :: a -> a -> Boolean

data Token = Token

derive instance eqToken :: Eq Token

main :: Int
main = 0
"#;
    let reexport_eq = r#"module Reexport (module Data.Eq) where

import Data.Eq
"#;
    let reexport_eq_main = r#"module Main where

import Reexport

data Token = Low | High

derive instance eqToken :: Eq Token

main :: Int
main = 0
"#;
    let empty_class_derive = r#"module Main where

class Marker a

data Box = Box

derive instance markerBox :: Marker Box

main :: Int
main = 0
"#;
    let empty_class_newtype_missing = r#"module Main where

class Marker a

newtype Age = Age Int

derive newtype instance markerAge :: Marker Age

main :: Int
main = 0
"#;
    let empty_class_newtype_valid = r#"module Main where

class Marker a

instance markerInt :: Marker Int

newtype Age = Age Int

derive newtype instance markerAge :: Marker Age

main :: Int
main = 0
"#;
    let structural_sources = [("Data.Eq.purs", eq_module), ("Main.purs", structural_eq)];
    let newtype_sources = [("Main.purs", newtype)];
    let higher_kinded_sources = [("Main.purs", higher_kinded_newtype)];
    let structural_ord_sources = [
        ("Data.Ordering.purs", ordering),
        ("Data.Ord.purs", ord),
        ("Main.purs", structural_ord),
    ];
    let nested_functor_sources = [
        ("Data.Functor.purs", functor_module),
        ("Main.purs", nested_functor),
    ];
    let functor_function_result_sources = [
        ("Data.Functor.purs", functor_module),
        ("Main.purs", functor_function_result),
    ];
    let functor_nested_function_array_sources = [
        ("Data.Functor.purs", functor_module),
        ("Main.purs", functor_nested_function_array),
    ];
    let contravariant_functor_sources = [
        ("Data.Functor.purs", functor_module),
        ("Main.purs", contravariant_functor),
    ];
    let bifunctor_pair_sources = [
        ("Data.Bifunctor.purs", bifunctor_module),
        ("Main.purs", bifunctor_pair),
    ];
    let invalid_bifunctor_sources = [
        ("Data.Bifunctor.purs", bifunctor_module),
        ("Main.purs", invalid_bifunctor),
    ];
    let contravariant_function_sources = [
        ("Data.Profunctor.purs", profunctor_module),
        ("Data.Functor.Contravariant.purs", contravariant_module),
        ("Main.purs", contravariant_function),
    ];
    let invalid_contravariant_sources = [
        ("Data.Functor.Contravariant.purs", contravariant_module),
        ("Main.purs", invalid_contravariant),
    ];
    let custom_prelude_sources = [
        ("Prelude.purs", custom_prelude_eq),
        ("Main.purs", custom_prelude_eq_main),
    ];
    let user_eq_sources = [("Main.purs", user_eq)];
    let reexport_eq_sources = [
        ("Data.Eq.purs", eq_module),
        ("Reexport.purs", reexport_eq),
        ("Main.purs", reexport_eq_main),
    ];
    let empty_class_sources = [("Main.purs", empty_class_derive)];
    let empty_class_newtype_missing_sources = [("Main.purs", empty_class_newtype_missing)];
    let empty_class_newtype_valid_sources = [("Main.purs", empty_class_newtype_valid)];
    let cases = [
        (
            "deriving-structural-eq",
            structural_sources.as_slice(),
            true,
        ),
        (
            "deriving-newtype-method-adapter",
            newtype_sources.as_slice(),
            true,
        ),
        (
            "deriving-higher-kinded-newtype",
            higher_kinded_sources.as_slice(),
            true,
        ),
        (
            "deriving-structural-ord",
            structural_ord_sources.as_slice(),
            true,
        ),
        (
            "deriving-functor-nested-field",
            nested_functor_sources.as_slice(),
            true,
        ),
        (
            "deriving-functor-recurses-through-function-result",
            functor_function_result_sources.as_slice(),
            true,
        ),
        (
            "deriving-functor-recurses-through-nested-function-and-array-results",
            functor_nested_function_array_sources.as_slice(),
            true,
        ),
        (
            "deriving-functor-rejects-contravariant-field",
            contravariant_functor_sources.as_slice(),
            false,
        ),
        (
            "deriving-bifunctor-final-two-parameters",
            bifunctor_pair_sources.as_slice(),
            true,
        ),
        (
            "deriving-bifunctor-rejects-negative-parameter",
            invalid_bifunctor_sources.as_slice(),
            false,
        ),
        (
            "deriving-contravariant-function-input",
            contravariant_function_sources.as_slice(),
            true,
        ),
        (
            "deriving-contravariant-rejects-positive-parameter",
            invalid_contravariant_sources.as_slice(),
            false,
        ),
        (
            "deriving-custom-prelude-eq-is-not-a-known-class",
            custom_prelude_sources.as_slice(),
            false,
        ),
        (
            "deriving-user-eq-is-not-selected-by-unqualified-name",
            user_eq_sources.as_slice(),
            false,
        ),
        (
            "deriving-recognizes-the-reexported-data-eq-identity",
            reexport_eq_sources.as_slice(),
            true,
        ),
        (
            "deriving-empty-unknown-class-is-rejected",
            empty_class_sources.as_slice(),
            false,
        ),
        (
            "derive-newtype-empty-class-requires-underlying-instance",
            empty_class_newtype_missing_sources.as_slice(),
            false,
        ),
        (
            "derive-newtype-empty-class-uses-underlying-instance",
            empty_class_newtype_valid_sources.as_slice(),
            true,
        ),
    ];
    let mut failures = Vec::new();
    for (name, sources, expected_acceptance) in cases {
        let purs_accepted = purs_accepts_sources(name, sources);
        let psrs_result = psrs_driver::check_program(sources);
        let psrs_accepted = psrs_result.is_ok();
        if purs_accepted != expected_acceptance {
            failures.push(format!(
                "`{name}`: purs accepted={purs_accepted}, expected={expected_acceptance}"
            ));
        }
        if psrs_accepted != expected_acceptance {
            let diagnostic = psrs_result
                .err()
                .map(|errors| format!(": {errors:?}"))
                .unwrap_or_default();
            failures.push(format!(
                "`{name}`: psrs accepted={psrs_accepted}, expected={expected_acceptance}{diagnostic}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "deriving differential failures:\n{}",
        failures.join("\n")
    );
}
