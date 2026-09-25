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
fn reports_a_missing_nested_constructor_as_a_coverage_witness() {
    let source = "\
module Main where
data Inner = First | Second
data Outer = Wrap Inner
main = case Wrap First of
  Wrap First -> 1
";
    let errors = compile_source("Main.purs", source).expect_err("non-exhaustive nested case");
    assert!(errors.iter().any(|error| {
        error.stage == "P8 closure conversion"
            && error.message.contains("missing pattern Wrap Second")
    }));
}

#[test]
fn exposes_redundant_case_alternatives_as_source_spanned_warnings() {
    let source = "module Main where\ndata Choice = First | Second\nchoose input = case input of\n  First -> 1\n  First -> 2\n  _ -> 3\nmain = choose First\n";
    let artifact = compile_source("Main.purs", source).expect("redundancy is a warning");
    assert_eq!(artifact.warnings.len(), 1);
    let warning = &artifact.warnings[0];
    assert_eq!(warning.source, 0);
    assert_eq!(warning.diagnostic.stage, "P8 closure conversion");
    assert!(
        warning
            .diagnostic
            .message
            .contains("redundant case alternative")
    );
    let warned_source =
        &source[warning.diagnostic.span.start as usize..warning.diagnostic.span.end as usize];
    assert_eq!(warned_source, "First -> 2");
}

#[test]
fn compiles_a_wildcard_case_over_a_recursive_adt() {
    let source = "\
module Main where
data List = Cons List | Nil
main = case Cons Nil of
  _ -> 42
";
    compile_source("Main.purs", source).expect("wildcard coverage over a recursive ADT");
}

#[test]
fn runs_a_case_on_nullary_constructors_when_wasmtime_is_available() {
    let artifact = compile_source("Main.purs", ENUM_SOURCE).expect("lowering enum cases");
    assert!(artifact.wat.contains("br_table"));
    let Some(output) = run_with_wasmtime(ENUM_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    // Red -> Green -> 2.
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn lowers_enum_case_to_mir_switch_and_wasm_br_table() {
    let stages =
        psrs_backend::compile_with_stages(lower_source_to_core("Main.purs", ENUM_SOURCE).unwrap())
            .unwrap();
    let has_tag_switch = stages
        .cc
        .functions
        .iter()
        .flat_map(|function| &function.assignments)
        .any(|assignment| {
            matches!(
                assignment.kind,
                psrs_backend::cc::AssignmentKind::TagSwitch { .. }
            )
        });
    assert!(has_tag_switch, "expected a tag switch in CC");
    let optimized_switch_is_well_formed = stages.mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            let Some(psrs_backend::mir::Terminator::Switch {
                value,
                cases,
                default,
                ..
            }) = &block.terminator
            else {
                return false;
            };
            function.values.iter().any(|decl| decl.id == *value)
                && cases.iter().all(|(_, target)| {
                    function
                        .blocks
                        .iter()
                        .any(|candidate| candidate.id == *target)
                })
                && function
                    .blocks
                    .iter()
                    .any(|candidate| candidate.id == *default)
        })
    });
    assert!(
        optimized_switch_is_well_formed,
        "P10 must preserve the selector and every MIR Switch successor"
    );
    assert!(stages.artifact.wat.contains("br_table"));
}

#[test]
fn dispatches_reordered_enum_tags_and_the_default_arm_correctly() {
    let source = "\
module Main where
data Color = Red | Green | Blue
toInt color = case color of
  Blue -> 30
  Red -> 10
  Green -> 20
main = toInt Green
";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(20));
}

#[test]
fn preserves_an_earlier_wildcard_before_a_constructor_pattern() {
    let source = "module Main where\ndata T = A | B\nmain = case A of\n  _ -> 10\n  A -> 20\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(10));
}

#[test]
fn preserves_an_earlier_variable_before_a_constructor_pattern() {
    let source = "module Main where\ndata T = A | B\nmain = case A of\n  value -> 10\n  A -> 20\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(10));
}

#[test]
fn preserves_the_first_of_duplicate_constructor_patterns() {
    let source =
        "module Main where\ndata T = A | B\nmain = case A of\n  A -> 10\n  A -> 20\n  _ -> 30\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(10));
}

#[test]
fn keeps_a_constructor_before_an_earlier_matching_wildcard() {
    let source =
        "module Main where\ndata T = A | B\nmain = case B of\n  A -> 10\n  _ -> 20\n  B -> 30\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(20));
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
        stages
            .cc
            .representations
            .representations
            .iter()
            .all(|representation| matches!(
                representation,
                psrs_backend::cc::Representation::Box {
                    value: psrs_backend::cc::ValueShape::Integer
                }
            )),
        "newtypes must not allocate target representations"
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
fn runs_a_parameterized_adt_with_an_erased_number_field() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nfromJust :: Maybe Number -> Number\nfromJust value = case value of\n  Just number -> number\n  _ -> 0.0\nuse :: Number -> Int\nuse value = 42\nmain = use (fromJust (Just 1.5))\n";
    let artifact = compile_source("Main.purs", source).expect("lowering Maybe Number");
    assert!(artifact.wat.contains("f64.const"));
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
fn runs_a_polymorphic_parameterized_declaration() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nid :: forall a. Maybe a -> Maybe a\nid x = x\nfromJust :: Maybe Int -> Int\nfromJust value = case value of\n  Just number -> number\n  _ -> 0\nmain = fromJust (id (Just 42))\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}
