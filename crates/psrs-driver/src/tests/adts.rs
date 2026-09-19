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
fn reports_field_constructor_patterns_as_a_limitation() {
    let source = "\
module Main where
data Maybe a = Nothing | Just a
f :: Maybe Int -> Int
f m = case m of
  Nothing -> 0
  Just x -> x
";
    let errors = compile_source("Main.purs", source).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("constructor patterns with fields")),
        "{errors:?}"
    );
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
