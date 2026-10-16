use super::*;

#[test]
fn public_number_from_string_executes_the_official_uncurried_wrapper() {
    let source = r#"
module Main where
import Prelude
import Data.Number as Number
import Data.Maybe (Maybe(..))
main :: Int
main = case Number.fromString "42.5" of
  Just value -> if value == 42.5 then 42 else 1
  Nothing -> 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn complete_decimal_conversion_preserves_rounding_range_and_zero_signs() {
    let source = r#"
module Main where
foreign import "psrs:intrinsic#numberFromDecimal" parse :: String -> Number
foreign import "psrs:intrinsic#numberEq" equal :: Number -> Number -> Boolean
foreign import "psrs:intrinsic#numberDiv" divide :: Number -> Number -> Number
foreign import "psrs:intrinsic#numberNeg" negative :: Number -> Number
foreign import "psrs:intrinsic#booleanAnd" both :: Boolean -> Boolean -> Boolean
foreign import "psrs:intrinsic#numberNe" unequal :: Number -> Number -> Boolean
invalid value = let number = parse value in unequal number number
precision = both (equal (parse "9007199254740993") 9007199254740992.0)
  (equal (parse "1.00000000000000011102230246251565404236316680908203125") 1.0)
range = both (equal (parse "5e-324") 5.0e-324) (equal (parse "1e309") (divide 1.0 0.0))
zeros = equal (divide 1.0 (parse "-1e-9999")) (divide (negative 1.0) 0.0)
grammar = both (invalid "42.5tail") (both (invalid "Infinity") (both (invalid " 42.5") (invalid "1e+")))
main = if both precision (both range (both zeros grammar)) then 42 else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn decimal_foreign_bindings_require_string_operand_and_number_result() {
    for ty in [
        "Int -> Number",
        "String -> Int",
        "Number -> Number",
        "forall a. a -> a",
    ] {
        let source = format!(
            "module Main where\nforeign import \"psrs:intrinsic#numberFromDecimal\" parse :: {ty}\nmain = 0\n"
        );
        let errors = compile_program_sources(&[("Main.purs", &source)])
            .expect_err("invalid decimal primitive contract");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.stage == "P8 primitive linking")
        );
    }
}

#[test]
fn explicit_type_applications_retain_universal_newtype_arguments() {
    let source = r#"
module Main where
newtype Wrapper a b = Wrapper (a -> b)
make :: forall @a @b. (a -> b) -> Wrapper a b
make f = Wrapper f
makePoly :: forall @b. ((forall a. a -> a) -> b) -> Wrapper (forall a. a -> a) b
makePoly = make @(forall a. a -> a)
apply :: (forall a. a -> a) -> Int
apply f = if f true then f 42 else 1
wrapped :: Wrapper (forall a. a -> a) Int
wrapped = makePoly @Int apply
run :: forall a b. Wrapper a b -> a -> b
run (Wrapper f) value = f value
main :: Int
main = run wrapped (\x -> x)
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    let invalid = r#"
module Main where
data Box a = Box a
make :: forall @a. a -> Box a
make value = Box value
wrapped :: Box (forall a. a -> a)
wrapped = make @(forall a. a -> a) (\x -> 42)
main :: Int
main = 42
"#;
    assert!(compile_source("Main.purs", invalid).is_err());
}
