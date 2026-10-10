use super::*;

#[test]
fn differential_remaining_deriving_rules_against_purs() {
    if !purs_available() {
        return;
    }
    let libraries = [
        (
            "Control.Category.purs",
            "module Control.Category where\nidentity :: forall a. a -> a\nidentity x = x\n",
        ),
        (
            "Data.Function.purs",
            r#"module Data.Function where

identity :: forall a. a -> a
identity x = x
"#,
        ),
        (
            "Data.Semigroup.purs",
            r#"module Data.Semigroup where

class Semigroup a where
  append :: a -> a -> a
"#,
        ),
        (
            "Data.Monoid.purs",
            r#"module Data.Monoid where

import Data.Semigroup

class Semigroup a <= Monoid a where
  mempty :: a
"#,
        ),
        (
            "Data.Functor.purs",
            r#"module Data.Functor where

class Functor f where
  map :: forall a b. (a -> b) -> f a -> f b

instance functorFunction :: Functor ((->) r) where
  map f g x = f (g x)
"#,
        ),
        (
            "Control.Apply.purs",
            r#"module Control.Apply where

import Data.Functor

class Functor f <= Apply f where
  apply :: forall a b. f (a -> b) -> f a -> f b
"#,
        ),
        (
            "Control.Applicative.purs",
            r#"module Control.Applicative where

import Control.Apply

class Apply f <= Applicative f where
  pure :: forall a. a -> f a
"#,
        ),
        (
            "Data.Foldable.purs",
            r#"module Data.Foldable where

import Data.Monoid

class Foldable t where
  foldr :: forall a b. (a -> b -> b) -> b -> t a -> b
  foldl :: forall a b. (b -> a -> b) -> b -> t a -> b
  foldMap :: forall a m. Monoid m => (a -> m) -> t a -> m
"#,
        ),
        (
            "Data.Bifoldable.purs",
            r#"module Data.Bifoldable where

import Data.Monoid

class Bifoldable p where
  bifoldr :: forall a b c. (a -> c -> c) -> (b -> c -> c) -> c -> p a b -> c
  bifoldl :: forall a b c. (c -> a -> c) -> (c -> b -> c) -> c -> p a b -> c
  bifoldMap :: forall m a b. Monoid m => (a -> m) -> (b -> m) -> p a b -> m
"#,
        ),
        (
            "Data.Traversable.purs",
            r#"module Data.Traversable where

import Control.Applicative
import Data.Functor
import Data.Function

class Traversable t where
  traverse :: forall a b f. Applicative f => (a -> f b) -> t a -> f (t b)
  sequence :: forall a f. Applicative f => t (f a) -> f (t a)
"#,
        ),
        (
            "Data.Bitraversable.purs",
            r#"module Data.Bitraversable where

import Control.Applicative
import Data.Functor
import Data.Function

class Bitraversable t where
  bitraverse :: forall f a b c d. Applicative f => (a -> f c) -> (b -> f d) -> t a b -> f (t c d)
  bisequence :: forall f a b. Applicative f => t (f a) (f b) -> f (t a b)
"#,
        ),
        (
            "Data.Profunctor.purs",
            r#"module Data.Profunctor where

class Profunctor p where
  dimap :: forall a b c d. (a -> b) -> (c -> d) -> p b c -> p a d
"#,
        ),
        (
            "Data.Functor.Contravariant.purs",
            r#"module Data.Functor.Contravariant where

class Contravariant f where
  cmap :: forall a b. (b -> a) -> f a -> f b
"#,
        ),
        (
            "Data.Bifunctor.purs",
            r#"module Data.Bifunctor where

class Bifunctor f where
  bimap :: forall a b c d. (a -> b) -> (c -> d) -> f a c -> f b d
"#,
        ),
    ];
    let cases = [
        (
            "Functor",
            "Data.Functor",
            "a",
            "{ field :: a, inert :: Int }",
            true,
        ),
        (
            "Bifunctor",
            "Data.Bifunctor",
            "a b",
            "{ left :: a, right :: b, inert :: Int }",
            true,
        ),
        (
            "Contravariant",
            "Data.Functor.Contravariant",
            "a",
            "a",
            false,
        ),
        ("Profunctor", "Data.Profunctor", "a b", "b", true),
        ("Profunctor", "Data.Profunctor", "a b", "a", false),
        (
            "Foldable",
            "Data.Foldable",
            "a",
            "{ field :: a, inert :: Int }",
            true,
        ),
        ("Foldable", "Data.Foldable", "a", "(a -> Int)", false),
        (
            "Bifoldable",
            "Data.Bifoldable",
            "a b",
            "{ left :: a, right :: b }",
            true,
        ),
        ("Bifoldable", "Data.Bifoldable", "a b", "(a -> Int)", false),
        (
            "Traversable",
            "Data.Traversable",
            "a",
            "{ field :: a, inert :: Int }",
            true,
        ),
        ("Traversable", "Data.Traversable", "a", "(a -> Int)", false),
        (
            "Bitraversable",
            "Data.Bitraversable",
            "a b",
            "{ left :: a, right :: b }",
            true,
        ),
        (
            "Bitraversable",
            "Data.Bitraversable",
            "a b",
            "(a -> Int)",
            false,
        ),
    ];
    let mut failures = Vec::new();
    for (index, (class, module, parameters, field, accepted)) in cases.into_iter().enumerate() {
        let main = format!(
            "module Main where\nimport Data.Function as Function\nimport Control.Category (identity)\nimport Data.Functor\nimport Control.Apply\nimport Control.Applicative\nimport {module}\ndata Box {parameters} = Box {field}\nderive instance derivedBox :: {class} Box\nmain = 0\n"
        );
        let mut sources = libraries.to_vec();
        sources.push(("Main.purs", &main));
        let name = format!("deriving-{class}-{index}");
        let purs_output = purs_sources_output(&name, &sources);
        let purs_accepted = purs_output.status.success();
        let psrs = psrs_driver::check_program(&sources);
        if purs_accepted != accepted || psrs.is_ok() != accepted {
            failures.push(format!(
                "{name}: expected={accepted}, purs={purs_accepted}, psrs={psrs:?}, purs stderr={}, stdout={}",
                String::from_utf8_lossy(&purs_output.stderr),
                String::from_utf8_lossy(&purs_output.stdout)
            ));
        }
        if !accepted {
            let codes = purs_error_codes(&purs_output);
            if !codes
                .iter()
                .any(|code| code == "CannotDeriveInvalidConstructorArg")
                || !psrs.as_ref().err().is_some_and(|errors| {
                    errors.iter().any(|error| {
                        error.diagnostic.code == Some("CannotDeriveInvalidConstructorArg")
                    })
                })
            {
                failures.push(format!(
                    "{name}: expected field-usage errorCode, purs={codes:?}, psrs={psrs:?}"
                ));
            }
        }
    }
    for (class, module) in [
        ("Functor", "Data.Functor"),
        ("Foldable", "Data.Foldable"),
        ("Traversable", "Data.Traversable"),
    ] {
        let main = format!(
            "module Main where\nimport Data.Function as Function\nimport Control.Category (identity)\nimport Data.Functor\nimport Control.Apply\nimport Control.Applicative\nimport {module}\ndata Box f a = Box (f a)\nderive instance boxInstance :: {class} f => {class} (Box f)\nmain = 0\n"
        );
        let mut sources = libraries.to_vec();
        sources.push(("Main.purs", &main));
        let output = purs_sources_output(&format!("deriving-context-{class}"), &sources);
        let psrs = psrs_driver::check_program(&sources);
        if !output.status.success() || psrs.is_err() {
            failures.push(format!("{class} context: purs={output:?}, psrs={psrs:?}"));
        }
    }
    let scoped = "module Main where\nimport Data.Bifunctor\nimport Data.Functor\ndata Box a b = Box (forall a. a -> b)\nderive instance bifunctorBox :: Bifunctor Box\nmain = 0\n";
    let mut sources = libraries.to_vec();
    sources.push(("Main.purs", scoped));
    let output = purs_sources_output("deriving-scoped-parameter", &sources);
    let psrs = psrs_driver::check_program(&sources);
    if !output.status.success() || psrs.is_err() {
        failures.push(format!("scoped parameter: purs={output:?}, psrs={psrs:?}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn differential_record_postfix_precedence_against_purs() {
    if !purs_available() {
        return;
    }
    let source = r#"module Main where
consume :: Int -> Int
consume x = x
project :: { field :: Int } -> Int
project r = consume r.field
consumeRecord :: { field :: Int } -> { field :: Int }
consumeRecord r = r
update :: { field :: Int } -> { field :: Int }
update r = consumeRecord r { field = 42 }
main :: Int
main = project (update { field: 0 })
"#;
    let sources = [("Main.purs", source)];
    let output = purs_sources_output("record-postfix-precedence", &sources);
    assert!(output.status.success(), "{output:?}");
    psrs_driver::check_program(&sources).expect("record postfix operands agree with purs");
}
