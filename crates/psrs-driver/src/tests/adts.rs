use super::*;

const ENUM_SOURCE: &str = "\
module Main where

data Color = Red | Green | Blue

next :: Color -> Color
next c = case c of
  Red -> Green
  Green -> Blue
  Blue -> Red

toInt :: Color -> Int
toInt c = case c of
  Red -> 1
  Green -> 2
  Blue -> 3

main :: Int
main = toInt (next Red)
";

#[test]
fn runs_a_case_on_nullary_constructors_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(ENUM_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    // Red -> Green -> 2.
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn lowers_enum_case_to_tag_comparisons() {
    let stages =
        psrs_backend::compile_with_stages(lower_source_to_core("Main.purs", ENUM_SOURCE).unwrap())
            .unwrap();
    let has_primitive = stages
        .cc
        .functions
        .iter()
        .flat_map(|function| &function.assignments)
        .any(|assignment| {
            matches!(
                assignment.kind,
                psrs_backend::cc::AssignmentKind::Primitive { .. }
            )
        });
    assert!(has_primitive, "expected tag comparison assignments");
}

#[test]
fn compiles_a_non_parameterized_field_constructor_case() {
    let source = "\
module Main where
data Pair = Pair Int Int | Empty
sum p = case p of
  Pair x y -> x + y
  Empty -> 0
main = sum (Pair 20 22)
";
    let artifact = compile_source("Main.purs", source).expect("lowering a GC constructor");
    assert!(artifact.wat.contains("struct.new"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_the_nullary_fallback_of_a_gc_constructor_case() {
    let source = "\
module Main where
data Pair = Pair Int Int | Empty
sum p = case p of
  Pair x y -> x + y
  Empty -> 0
main = sum Empty
";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn runs_nested_non_parameterized_gc_aggregates() {
    let source = "\
module Main where
data Inner = Inner Int
data Outer = Outer Inner
unwrap value = case value of
  Outer inner -> case inner of
    Inner number -> number
main = unwrap (Outer (Inner 42))
";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_nested_constructor_pattern_in_a_gc_field() {
    let source = "\
module Main where
data Inner = Inner Int
data Outer = Outer Inner
unwrap value = case value of
  Outer (Inner number) -> number
main = unwrap (Outer (Inner 42))
";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn selects_between_nested_constructor_patterns() {
    let source = r#"
module Main where
data Inner = Left Int | Right Int
data Outer = Outer Inner
unwrap value = case value of
  Outer (Left number) -> number
  Outer (Right number) -> number + 1
main = unwrap (Outer (Right 41))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn selects_the_first_nested_constructor_pattern() {
    let source = r#"
module Main where
data Inner = Left Int | Right Int
data Outer = Outer Inner
unwrap value = case value of
  Outer (Left number) -> number
  Outer (Right number) -> number + 1
main = unwrap (Outer (Left 41))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(41));
}

#[test]
fn erases_a_newtype_constructor_and_pattern_at_runtime() {
    let source = "\
module Main where
newtype Age = Age Int
unAge (Age number) = number
main = unAge (Age 42)
";
    let stages =
        psrs_backend::compile_with_stages(lower_source_to_core("Main.purs", source).unwrap())
            .unwrap();
    assert!(
        stages.cc.types.is_empty(),
        "newtypes must not allocate GC types"
    );
    assert!(
        !stages.artifact.wat.contains("struct.new"),
        "newtype construction should pass through its field"
    );
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn selects_nested_patterns_through_an_erased_newtype() {
    let source = r#"
module Main where
data Inner = Left Int | Right Int
newtype Box = Box Inner
unwrap value = case value of
  Box (Left number) -> number
  Box (Right number) -> number + 1
main = unwrap (Box (Right 41))
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_constructor_pattern_in_a_function_argument() {
    let source = "\
module Main where
data Pair = Pair Int Int
sum (Pair left right) = left + right
main = sum (Pair 20 22)
";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_parameterized_adt_with_an_erased_scalar_field() {
    let source = "\
module Main where
data Maybe a = Nothing | Just a
fromJust :: Maybe Int -> Int
fromJust value = case value of
  Just number -> number
  _ -> 0
main = fromJust (Just 42)
";
    let artifact = compile_source("Main.purs", source).expect("lowering Maybe Int");
    assert!(artifact.wat.contains("struct.new"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn runs_a_nested_pattern_on_an_erased_parameterized_field() {
    let source = "\
module Main where
data Maybe a = Nothing | Just a
fromJust :: Maybe (Maybe Int) -> Int
fromJust value = case value of
  Just (Just number) -> number
  _ -> 0
main = fromJust (Just (Just 42))
";
    let artifact = compile_source("Main.purs", source).expect("lowering nested Maybe");
    assert!(artifact.wat.contains("struct.new"));
    assert!(artifact.wat.contains("ref.cast"));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn falls_back_when_a_nested_erased_pattern_does_not_match() {
    let source = "\
module Main where
data Maybe a = Nothing | Just a
fromJust :: Maybe (Maybe Int) -> Int
fromJust value = case value of
  Just (Just number) -> number
  _ -> 7
main = fromJust (Just Nothing)
";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(7));
}

#[test]
fn reports_uninstantiated_parameterized_declarations_as_a_limitation() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nid :: forall a. Maybe a -> Maybe a\nid x = x\nmain = id (Just 0)\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("polymorphic declarations")),
        "{errors:?}"
    );
}
