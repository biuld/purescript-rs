//! A written annotation's binders scope its body before expected-type adaptation.

pub const SINGLE: &str = r#"module Main where
identity = ((\value -> (value :: b)) :: b -> b) :: forall b. b -> b
main :: Int
main = if identity true then identity 42 else 1
"#;

pub const NESTED: &str = r#"module Main where
identity = ((((\value -> (value :: b)) :: b -> b) :: forall b. b -> b) :: forall a. a -> a)
main :: Int
main = if identity true then identity 42 else 1
"#;

pub const RECORD: &str = r#"module Main where
box :: { run :: forall a. a -> a }
box = {run: ((\value -> (value :: b)) :: b -> b) :: forall b. b -> b}
main :: Int
main = if box.run true then box.run 42 else 1
"#;

pub const QUALIFIED: &str = r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
instance markBoolean :: Mark Boolean where
  mark flag = if flag then 42 else 1
use :: (forall a. Mark a => a -> Int) -> Int
use f = case f true of
  42 -> f 42
  _ -> 1
main :: Int
main = use (((\value -> mark (value :: b)) :: b -> Int) :: forall b. Mark b => b -> Int)
"#;

pub const SPECIALIZED: &str = r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
main :: Int
main = (((\value -> mark (value :: b)) :: b -> Int) :: forall b. Mark b => b -> Int) 42
"#;

pub const VISIBLE: &str = r#"module Main where
main :: Int
main = ((((\value -> (value :: b)) :: b -> b) :: forall @b. b -> b) @Int) 42
"#;

pub const PARTIAL: &str = r#"module Main where
choose = (((\ignored value -> (value :: b)) :: a -> b -> b) :: forall @a b. a -> b -> b) @Boolean
main :: Int
main = if choose false true then choose true 42 else 1
"#;

pub const SKIPPED: &str = r#"module Main where
main :: Int
main = (((((\ignored value -> (value :: b)) :: a -> b -> b) :: forall @a @b. a -> b -> b) @_) @Int) true 42
"#;

pub const SHADOWED: &str = r#"module Main where
outer :: forall a. a -> Int
outer ignored = let
  identity = ((\value -> (value :: a)) :: a -> a) :: forall a. a -> a
  in if identity true then identity 42 else 1
main :: Int
main = outer "outside"
"#;

pub const SCOPE_EXIT: &str = r#"module Main where
identity = (\value -> (value :: a)) :: forall a. a -> a
bad value = (value :: a)
main :: Int
main = 42
"#;

pub const RIGID: &str = r#"module Main where
bad :: Int -> (forall a. a -> a)
bad number = ((\ignored -> number) :: forall b. b -> b)
main :: Int
main = 42
"#;

pub const IMPORTED: &[(&str, &str)] = &[
    (
        "Poly.purs",
        r#"module Poly where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
instance markBoolean :: Mark Boolean where
  mark flag = if flag then 42 else 1
identity = ((\value -> (value :: b)) :: b -> b) :: forall b. b -> b
choose :: forall a. Mark a => a -> Int
choose = ((\value -> mark (value :: b)) :: b -> Int) :: forall b. Mark b => b -> Int
"#,
    ),
    (
        "Main.purs",
        r#"module Main where
import Poly (identity, choose)
main :: Int
main = if identity true then case choose true of
  42 -> choose (identity 42)
  _ -> 1
  else 1
"#,
    ),
];

pub const GIVEN_SKOLEM: &str = r#"module Main where
class Determine a | -> a where
  determine :: a -> Int
instance determineInt :: Determine Int where
  determine value = value
wrapped = (\ignored -> let
  bad = (\value -> determine value) :: forall b. b -> Int
  in 42) :: forall a. Determine a => a -> Int
main :: Int
main = 42
"#;
