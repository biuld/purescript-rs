use super::*;

#[test]
fn generic_deriving_round_trips_constructor_tags_and_fields() {
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

data Choice a = Empty | Single a | Pair a Int

derive instance genericChoice :: Generic (Choice a) _

main :: Int
main = case to (from (Pair 40 2)) of
  Pair x y -> case to (from (Single 7)) of
    Single z -> if intEq z 7 then case to (from (Empty :: Choice Int)) of
      Empty -> intAdd x y
      _ -> 0
      else 1
    _ -> 2
  _ -> 3
"#;
    let Some(output) =
        run_program_with_wasmtime(&[("Data.Generic.Rep.purs", generic_rep), ("Main.purs", main)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn generic_round_trip_uses_the_trusted_representation_declarations() {
    let source = r#"module Main where
import Data.Generic.Rep (class Generic, to, from)
data Choice a = Empty | Single a | Pair a Int
derive instance genericChoice :: Generic (Choice a) _
main :: Int
main = case to (from (Pair 40 2)) of
  Pair x y -> intAdd x y
  _ -> 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
