use super::*;

#[test]
fn eq_and_ord_expand_applied_variable_aliases_and_use_higher_kinded_dictionaries() {
    let eq = r#"module Data.Eq where
class Eq a where
  eq :: a -> a -> Boolean
class Eq1 f where
  eq1 :: forall a. Eq a => f a -> f a -> Boolean
instance eqInt :: Eq Int where
  eq x y = intEq x y
"#;
    let ordering = "module Data.Ordering where\ndata Ordering = LT | EQ | GT\n";
    let ord = r#"module Data.Ord where
import Data.Ordering
class Ord a where
  compare :: a -> a -> Ordering
class Ord1 f where
  compare1 :: forall a. Ord a => f a -> f a -> Ordering
instance ordInt :: Ord Int where
  compare x y = if intEq x y then EQ else if intLt x y then LT else GT
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

type Applied f a = f a
data Box f a = Box (Applied f a)
derive instance eqBox :: (Eq1 f, Eq a) => Eq (Box f a)
derive instance ordBox :: (Ord1 f, Ord a) => Ord (Box f a)

main :: Int
main = if eq (Box (Just 4)) (Box (Just 4)) then
  if eq (Box (Just 4)) (Box (Just 5)) then 1 else
    case compare (Box (Just 4)) (Box (Just 5)) of
      LT -> 42
      _ -> 2
  else 3
"#;
    let sources = [
        ("Data.Eq.purs", eq),
        ("Data.Ordering.purs", ordering),
        ("Data.Ord.purs", ord),
        ("Main.purs", main),
    ];
    crate::check_program(&sources).expect("applied-variable aliases should type check");
    let Some(output) = run_program_with_wasmtime(&sources) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}
