use super::run_with_wasmtime;

#[test]
fn an_explicit_partial_scope_returns_values_and_traps_on_missing_cases() {
    for (argument, expected) in [("Just 42", Some(42)), ("Nothing", None)] {
        let source = format!(
            r#"module Main where
import Partial.Unsafe (unsafePartial)
data Maybe a = Nothing | Just a
extract :: Partial => Maybe Int -> Int
extract (Just x) = x
main :: Int
main = unsafePartial (extract ({argument}))
"#
        );
        let Some(output) = run_with_wasmtime(&source) else {
            return;
        };
        if let Some(code) = expected {
            assert_eq!(output.status.code(), Some(code), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        } else {
            assert!(!output.status.success(), "{output:?}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("unreachable"),
                "{output:?}"
            );
        }
    }
}

#[test]
fn partial_application_calls_the_declaration_and_then_its_returned_function() {
    let source = r#"module Main where
data Box a = Box a
invoke :: forall a b c. Int -> Box (a -> b -> c) -> a -> b -> c
invoke _ (Box f) x y = f x y
add x y = intAdd x y
main :: Int
main =
  let pending = invoke 0
      result = pending (Box add) 40 2
  in result
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn polymorphic_constructor_fields_keep_application_quantifier_scope() {
    let source = r#"module Main where
data Maybe a = Nothing | Just a
newtype Parser a = Parser (Int -> Maybe a)
constant x _ = x
dictionary :: { empty :: forall a. Parser a }
dictionary = { empty: Parser (constant Nothing) }
main :: Int
main = case (dictionary.empty :: Parser Int) of
  Parser p -> case p 0 of
    Nothing -> 42
    Just _ -> 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn constructor_patterns_return_functions_at_the_checked_calling_boundary() {
    let source = r#"module Main where
newtype Fn2 a b c = Fn2 (a -> b -> c)
runFn2 :: forall a b c. Fn2 a b c -> a -> b -> c
runFn2 (Fn2 f) a b = f a b
alias = runFn2
add x y = intAdd x y
main :: Int
main =
  let wrapped = Fn2 add
      partial = runFn2 wrapped 40
      returned = runFn2 wrapped
      throughAlias = alias wrapped
  in if booleanAnd (intEq (runFn2 wrapped 40 2) 42)
    (booleanAnd (intEq (partial 2) 42)
    (booleanAnd (intEq (returned 40 2) 42)
    (intEq (throughAlias 40 2) 42))) then 42 else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn superclass_dictionary_construction_is_delayed_until_selection() {
    let source = r#"module Main where
class Base a where
  base :: a -> Int
class Base a <= Child a where
  child :: a -> Int
helper :: forall a. Child a => a -> Int
helper = child
instance baseInt :: Base Int where
  base = helper
instance childInt :: Child Int where
  child x = intAdd x 2
main :: Int
main = base 40
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn abstract_array_constructor_transport_converts_element_storage() {
    let source = r#"module Main where
class Build f where
  build :: forall a. Array a -> f a
  consume :: forall a. f a -> Array a
instance buildArray :: Build Array where
  build xs = xs
  consume xs = xs
roundTrip :: forall f. Build f => f Int -> Array Int
roundTrip = consume
main :: Int
main = case (roundTrip (build [42] :: Array Int)) of
  [x] -> x
  _ -> 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn generic_value_slots_preserve_nested_array_element_protocols() {
    let source = r#"module Main where
data Box a = Box a
box :: forall a. a -> Box a
box x = Box x
hold :: forall a. Array a -> Box (Array a)
hold xs = box xs
main :: Int
main = case hold [[42]] of
  Box [[x]] -> x
  _ -> 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn closed_uses_of_a_local_row_lambda_keep_distinct_fields_and_captures() {
    let source = r#"module Main where
main :: Int
main =
  let captured = 2
      select :: forall r. { value :: Int | r } -> Int
      select { value: x } = intAdd x captured
      first = select { value: 20, left: true }
      second = select { value: 18, right: [1,2] }
  in intAdd first second
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn generic_value_slots_preserve_records_with_array_fields() {
    let source = r#"module Main where
data Box a = Box a
box :: forall a. a -> Box a
box x = Box x
hold :: forall a. Array a -> Box { head :: a, tail :: Array a }
hold xs = box { head: arrayIndex xs 0, tail: xs }
main :: Int
main = case hold [42] of
  Box record -> if intEq record.head (arrayIndex record.tail 0) then record.head else 1
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}
