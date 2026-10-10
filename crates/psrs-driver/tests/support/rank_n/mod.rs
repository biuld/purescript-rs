//! Source cases shared by semantic, differential, and execution acceptance.

mod reify;

pub const CHECK_ONLY: &[(&str, &str)] = &[(
    "annotated_polymorphic_array",
    r#"module Main where
ids :: Array (forall a. a -> a)
ids = [\x -> x]
main :: Int
main = 42
"#,
)];

pub const LINKED: &[(&str, &str)] = &[
    (
        "Poly.purs",
        r#"module Poly where
type Identity = forall a. a -> a
data Holder = Holder Identity
make :: Int -> Identity
make ignored = \x -> x
"#,
    ),
    (
        "Main.purs",
        r#"module Main where
import Poly
main :: Int
main = case Holder (make 0) of
  Holder f -> if f true then f 42 else 1
"#,
    ),
];

pub const ACCEPT: &[(&str, &str)] = &[
    ("rank_n_separate_equations", reify::EQUATIONS),
    ("rank_n_multiple_scrutinees", reify::MULTIPLE),
    ("rank_n_guarded_equations", reify::GUARDED),
    (
        "returned_constraint",
        r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
make :: Int -> (forall a. Mark a => a -> Int)
make ignored = \value -> mark value
main :: Int
main = make 0 42
"#,
    ),
    (
        "constrained_data_field",
        r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
data Marker = Marker (forall a. Mark a => a -> Int)
main :: Int
main = case Marker (\value -> mark value) of
  Marker f -> f 42
"#,
    ),
    (
        "church_numerals",
        r#"module Main where
data Nat = Nat (forall r. r -> (r -> r) -> r)
data Count = C0 | C1 | C2 | C3 | C4
step :: Count -> Count
step count = case count of
  C0 -> C1
  C1 -> C2
  C2 -> C3
  C3 -> C4
  C4 -> C4
run :: Nat -> Count
run nat = case nat of
  Nat f -> f C0 step
zero = Nat (\z ignored -> z)
next n = case n of
  Nat f -> Nat (\z step -> step (f z step))
add n m = case n of
  Nat f -> case m of
    Nat g -> Nat (\z step -> g (f z step) step)
two = next (next zero)
main :: Int
main = case run (add two two) of
  C4 -> 42
  _ -> 1
"#,
    ),
    (
        "constrained_class_method",
        r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
class Lookup a where
  lookup :: forall b. Mark b => a -> b -> Int
instance lookupInt :: Lookup Int where
  lookup _ value = mark value
main :: Int
main = lookup 7 42
"#,
    ),
    (
        "partial_quantifier_instantiation_inferred",
        r#"module Main where
choose x y = y
use :: (forall a. Int -> a -> a) -> Int
use f = if f 0 true then f 0 42 else 1
main :: Int
main = use choose
"#,
    ),
    (
        "partial_quantifier_instantiation_explicit",
        r#"module Main where
choose :: forall a b. a -> b -> b
choose _ y = y
use :: (forall a. Int -> a -> a) -> Int
use f = if f 0 true then f 0 42 else 1
main :: Int
main = use choose
"#,
    ),
    (
        "vacuous_constraint_annotation",
        r#"module Main where
class Mark a where
  mark :: Int
bad :: forall a. Mark a => Int
bad = 42
main :: Int
main = 42
"#,
    ),
    (
        "returned_record_field",
        r#"module Main where
box :: { make :: Int -> (forall a. a -> a) }
box = { make: \ignored -> \x -> x }
main :: Int
main = if box.make 0 true then box.make 0 42 else 1
"#,
    ),
    (
        "returned_parameter",
        r#"module Main where
make :: Int -> (forall a. a -> a)
make ignored = \x -> x
use :: (Int -> (forall a. a -> a)) -> Int
use producer = if producer 0 true then producer 0 42 else 1
main :: Int
main = use make
"#,
    ),
    (
        "constrained_method_parameter",
        r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
class Apply a where
  apply :: a -> (forall b. Mark b => b -> Int) -> Int
instance applyInt :: Apply Int where
  apply value f = f value
main :: Int
main = apply 42 (\x -> mark x)
"#,
    ),
    (
        "application_argument",
        r#"module Main where
identity :: forall a. a -> a
identity x = x
use :: (forall a. a -> a) -> Int
use f = if f true then f 42 else 1
main :: Int
main = use (identity (\x -> x))
"#,
    ),
    (
        "nested_return_boundaries",
        r#"module Main where
make :: Int -> (forall a. a -> (forall b. b -> b))
make ignored outer = \x -> x
main :: Int
main = make 0 true 42
"#,
    ),
    (
        "record_subsumption",
        r#"module Main where
box :: { run :: forall a. a -> a }
box = { run: \x -> x }
use :: { run :: Int -> Int } -> Int
use b = b.run 42
main :: Int
main = use box
"#,
    ),
    (
        "function_variance",
        r#"module Main where
mono :: (Int -> Int) -> Int
mono f = f 42
outer :: ((forall a. a -> a) -> Int) -> Int
outer g = g (\x -> x)
main :: Int
main = outer mono
"#,
    ),
    (
        "higher_kinded_parameter",
        r#"module Main where
data Box a = Box a
data Other a = Other a
use :: (forall (f :: Type -> Type) a. f a -> f a) -> Int
use g = case g (Box true) of
  Box flag -> if flag then case g (Other 42) of
    Other value -> value
    else 1
main :: Int
main = use (\x -> x)
"#,
    ),
    (
        "conditional_argument",
        r#"module Main where
use :: (forall a. a -> a) -> Int
use f = if f true then f 42 else 1
main :: Int
main = use (if true then (\x -> x) else (\x -> x))
"#,
    ),
    (
        "let_argument",
        r#"module Main where
use :: (forall a. a -> a) -> Int
use f = if f true then f 42 else 1
main :: Int
main = use (let f = \x -> x in f)
"#,
    ),
    (
        "direct_return_application",
        r#"module Main where
make :: Int -> (forall a. a -> a)
make ignored = \x -> x
main :: Int
main = if make 0 true then make 0 42 else 1
"#,
    ),
    (
        "parameter",
        r#"module Main where
use :: (forall a. a -> a) -> Int
use f = if f true then f 42 else 1
main :: Int
main = use (\x -> x)
"#,
    ),
    (
        "rank_three",
        r#"module Main where
use :: (forall a. a -> a) -> Int
use f = if f true then f 42 else 1
outer :: ((forall a. a -> a) -> Int) -> Int
outer g = g (\x -> x)
main :: Int
main = outer use
"#,
    ),
    (
        "rank_four",
        r#"module Main where
use :: (forall a. a -> a) -> Int
use f = if f true then f 42 else 1
outer :: ((forall a. a -> a) -> Int) -> Int
outer g = g (\x -> x)
deep :: (((forall a. a -> a) -> Int) -> Int) -> Int
deep h = h use
main :: Int
main = deep outer
"#,
    ),
    (
        "record_field",
        r#"module Main where
box :: { run :: forall a. a -> a }
box = { run: \x -> x }
main :: Int
main = if box.run true then box.run 42 else 1
"#,
    ),
    (
        "record_pattern",
        r#"module Main where
use :: { run :: forall a. a -> a } -> Int
use { run: f } = if f true then f 42 else 1
main :: Int
main = use { run: \x -> x }
"#,
    ),
    (
        "data_field",
        r#"module Main where
data Identity = Identity (forall a. a -> a)
use :: Identity -> Int
use (Identity f) = if f true then f 42 else 1
main :: Int
main = use (Identity (\x -> x))
"#,
    ),
    (
        "newtype_field",
        r#"module Main where
newtype Identity = Identity (forall a. a -> a)
use :: Identity -> Int
use (Identity f) = if f true then f 42 else 1
main :: Int
main = use (Identity (\x -> x))
"#,
    ),
    (
        "returned_value",
        r#"module Main where
make :: Int -> (forall a. a -> a)
make ignored = \x -> x
main :: Int
main = let f = make 0 in if f true then f 42 else 1
"#,
    ),
    (
        "captured_value",
        r#"module Main where
capture :: (forall a. a -> a) -> Boolean -> Int
capture f flag = let g = \b -> if f b then f 42 else 1 in g flag
main :: Int
main = capture (\x -> x) true
"#,
    ),
    (
        "synonym",
        r#"module Main where
type Identity = forall a. a -> a
use :: Identity -> Int
use f = if f true then f 42 else 1
main :: Int
main = use (\x -> x)
"#,
    ),
    (
        "shadowed_binder",
        r#"module Main where
use :: forall a. a -> (forall a. a -> a) -> Int
use ignored f = if f true then f 42 else 1
main :: Int
main = use "outer" (\x -> x)
"#,
    ),
    (
        "recursive_signature",
        r#"module Main where
loop :: (forall a. a -> a) -> Boolean -> Int
loop f flag = if flag then loop f false else if f true then f 42 else 1
main :: Int
main = loop (\x -> x) true
"#,
    ),
    (
        "constrained_parameter",
        r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
instance markBoolean :: Mark Boolean where
  mark value = if value then 42 else 1
use :: (forall a. Mark a => a -> Int) -> Int
use f = if true then f 42 else f true
main :: Int
main = use mark
"#,
    ),
    (
        "constrained_lambda",
        r#"module Main where
class Mark a where
  mark :: a -> Int
instance markInt :: Mark Int where
  mark value = value
use :: (forall a. Mark a => a -> Int) -> Int
use f = f 42
main :: Int
main = use (\x -> mark x)
"#,
    ),
    (
        "class_method",
        r#"module Main where
class Apply a where
  apply :: a -> (forall b. b -> b) -> Int
instance applyInt :: Apply Int where
  apply value f = if f true then f value else 1
main :: Int
main = apply 42 (\x -> x)
"#,
    ),
];

mod reject;
pub use reject::REJECT;
