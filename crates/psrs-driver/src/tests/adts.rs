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
fn reports_parameterized_types_as_a_limitation() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nf :: Maybe Int -> Maybe Int\nf x = x\n";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("aggregate or parameterized types")),
        "{errors:?}"
    );
}
