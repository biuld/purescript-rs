//! Local aliases retain an existing quantified value; lambdas still infer monotypes.

pub const UNUSED: &str = r#"module Main where
class Mark a where
  mark :: a -> Int
main :: Int
main = let f = mark in 42
"#;

pub const DISTINCT: &str = r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
instance markBoolean :: Mark Boolean where
  mark value = if value then 42 else 1
main :: Int
main = let f = mark in case f true of
  42 -> f 42
  _ -> 1
"#;

pub const PARTIAL: &str = r#"module Main where
class Pick a b where
  pick :: a -> b -> Int
instance pickIntBoolean :: Pick Int Boolean where
  pick value flag = if flag then value else 1
instance pickIntInt :: Pick Int Int where
  pick ignored value = value
main :: Int
main = let f = pick @Int in case f 7 true of
  7 -> f 0 42
  _ -> 1
"#;

pub const CHAIN: &str = r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
instance markBoolean :: Mark Boolean where
  mark value = if value then 42 else 1
main :: Int
main = let f = mark
           g = f
       in case g true of
         42 -> g 42
         _ -> 1
"#;

pub const MISSING: &str = r#"module Main where
class Mark a where
  mark :: a -> Int
main :: Int
main = let f = mark in f 42
"#;

pub const ORDINARY: &str = r#"module Main where
identity :: forall a. a -> a
identity value = value
main :: Int
main = let f = identity in if f true then f 42 else 1
"#;

pub const GLOBAL: &str = r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
instance markBoolean :: Mark Boolean where
  mark value = if value then 42 else 1
qualified :: forall a. Mark a => a -> Int
qualified value = mark value
main :: Int
main = let f = qualified in case f true of
  42 -> f 42
  _ -> 1
"#;

pub const METHOD_PATTERN: &str = r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
instance markBoolean :: Mark Boolean where
  mark value = if value then 42 else 1
main :: Int
main = case mark of
  f -> case f true of
    42 -> f 42
    _ -> 1
"#;

pub const ORDINARY_PATTERN: &str = r#"module Main where
identity :: forall a. a -> a
identity value = value
main :: Int
main = case identity of
  f -> if f true then f 42 else 1
"#;

pub const IMPORTED: &[(&str, &str)] = &[
    (
        "Alias.purs",
        r#"module Alias where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
instance markBoolean :: Mark Boolean where
  mark value = if value then 42 else 1
qualified :: forall a. Mark a => a -> Int
qualified value = mark value
identity :: forall a. a -> a
identity value = value
"#,
    ),
    (
        "Main.purs",
        r#"module Main where
import Alias (identity, qualified)
main :: Int
main = let f = identity
           g = qualified
       in if f true then case g true of
         42 -> g (f 42)
         _ -> 1
       else 1
"#,
    ),
];

pub const RECORD_PROJECT: &str = r#"module Main where
box :: forall a. { run :: a -> a }
box = { run: \value -> value }
main :: Int
main = box.run 42
"#;

pub const RECORD_UPDATE: &str = r#"module Main where
config :: forall a. { count :: Int }
config = { count: 1 }
main :: Int
main = (config { count = 42 }).count
"#;

pub const CONDITION: &str = r#"module Main where
condition :: forall a. Boolean
condition = true
choose = if condition then 42 else 1
main :: Int
main = choose
"#;
