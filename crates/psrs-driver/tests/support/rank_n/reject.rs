pub const REJECT: &[(&str, &str)] = &[
    ("rank_n_record_shares_instantiation", super::reify::RECORD),
    (
        "ambiguous_unresolved_main_use",
        r#"module Main where
class Mark a where
  mark :: Int
instance markInt :: Mark Int where
  mark = 42
instance markBoolean :: Mark Boolean where
  mark = 1
main :: Int
main = mark
"#,
    ),
    (
        "specialized_class_method",
        r#"module Main where
class Identity a where
  identity :: a -> (forall b. b -> b)
instance identityInt :: Identity Int where
  identity ignored = \x -> 42
main :: Int
main = 42
"#,
    ),
    (
        "impredicative_constructor_instantiation",
        r#"module Main where
data Box a = Box a
box :: Box (forall a. a -> a)
box = Box (\x -> x)
main :: Int
main = 42
"#,
    ),
    (
        "inferred_parameter_escape",
        r#"module Main where
use :: (forall a. a -> a) -> Int
use f = f 42
test x = use x
main :: Int
main = 42
"#,
    ),
    (
        "monomorphic_parameter",
        r#"module Main where
use :: (forall a. a -> a) -> Int
use f = f 42
onlyInt :: Int -> Int
onlyInt x = x
main :: Int
main = use onlyInt
"#,
    ),
    (
        "specialized_lambda",
        r#"module Main where
use :: (forall a. a -> a) -> Int
use f = f 42
main :: Int
main = use (\x -> 42)
"#,
    ),
    (
        "skolem_escape",
        r#"module Main where
escape :: forall a. (forall b. b -> b) -> a
escape f = f 42
main :: Int
main = 42
"#,
    ),
    (
        "specialized_record",
        r#"module Main where
box :: { run :: forall a. a -> a }
box = { run: \x -> 42 }
main :: Int
main = 42
"#,
    ),
    (
        "specialized_data",
        r#"module Main where
data Identity = Identity (forall a. a -> a)
box :: Identity
box = Identity (\x -> 42)
main :: Int
main = 42
"#,
    ),
    (
        "function_subsumption_direction",
        r#"module Main where
use :: (forall a. a -> a) -> Int
use f = f 42
outer :: ((Int -> Int) -> Int) -> Int
outer g = g (\x -> 42)
main :: Int
main = outer use
"#,
    ),
];
