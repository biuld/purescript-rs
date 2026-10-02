use super::*;

#[test]
fn comma_guards_short_circuit_then_use_the_next_guarded_clause() {
    let source = "module Main where\nclassify n\n  | n > 0, n < 10 = 11\n  | true = 22\nmain = classify 12\n";
    let artifact = compile_source("Main.purs", source).expect("guarded equations compile");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(22), "{output:?}");
}

#[test]
fn failed_pattern_guard_falls_through_to_the_next_function_equation() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nread (Just n)\n  | n > 0 = n\nread _ = 41\nmain = read (Just (0 - 1))\n";
    let artifact = compile_source("Main.purs", source).expect("pattern guards compile");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(41), "{output:?}");
}

#[test]
fn case_alternative_pattern_guard_scopes_binders_and_falls_through() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nread value = case value of\n  candidate\n    | Just inner <- candidate, inner > 0 -> inner\n  _ -> 43\nmain = read (Just (0 - 1))\n";
    let artifact = compile_source("Main.purs", source).expect("case pattern guards compile");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(43), "{output:?}");
}

#[test]
fn guarded_helper_cases_do_not_emit_source_coverage_diagnostics() {
    let source = "module Main where\ndata Maybe a = Nothing | Just a\nchoose value = case value of\n  Just n | n > 0 -> n\n  _ -> 42\nmain = choose (Just (0 - 1))\n";
    let artifact = compile_source("Main.purs", source).expect("guard helper case compiles");
    assert!(
        artifact.warnings.iter().all(|warning| {
            !warning.diagnostic.message.contains("non-exhaustive case")
                && !warning
                    .diagnostic
                    .message
                    .contains("redundant case alternative")
        }),
        "generated guard continuations are not source coverage rows: {:?}",
        artifact.warnings
    );
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn record_pattern_guard_matches_a_record_held_in_a_local() {
    let source = "module Main where\nimport Prelude\nread :: { x :: Int } -> Int\nread value = case 0 of\n  _ | { x } <- value -> x\n  _ -> 44\nmain = read { x: 9 }\n";
    let artifact = compile_source("Main.purs", source).expect("record pattern guard compiles");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(9), "{output:?}");
}

#[test]
fn nested_boolean_record_pattern_guard_matches_through_a_compiler_test() {
    let source = "module Main where\nread :: { enabled :: Boolean } -> Int\nread value = case 0 of\n  _ | { enabled: true } <- value -> 9\n  _ -> 44\nmain = read { enabled: true }\n";
    let artifact = compile_source("Main.purs", source).expect("nested Boolean guard compiles");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(9), "{output:?}");
}

#[test]
fn boolean_case_patterns_preserve_source_order() {
    let source =
        "module Main where\nmain = case false of\n  true -> 11\n  false -> 22\n  _ -> 33\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(22), "{output:?}");
}

#[test]
fn anonymous_case_inputs_become_function_parameters_in_source_order() {
    let source = "module Main where\nchoose = case _, 2, _ of\n  _, 2, _ -> 19\n  _, _, _ -> 23\nmain = choose 1 3\n";
    let artifact = compile_source("Main.purs", source).expect("anonymous case inputs compile");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(19), "{output:?}");
}

#[test]
fn boolean_case_guards_preserve_alternative_fallthrough() {
    let source = "module Main where\nchoose input = case false of\n  true | input > 0 -> 11\n  _ -> 22\nmain = choose 0\n";
    let artifact = compile_source("Main.purs", source).expect("guarded Boolean cases compile");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(22), "{output:?}");
}

#[test]
fn integer_literal_patterns_fall_through_and_share_case_scrutinees() {
    let source = "module Main where\nchoose n = case n of\n  0 -> 11\n  1 -> 22\n  _ -> 33\nmain = choose 1\n";
    let artifact = compile_source("Main.purs", source).expect("integer case patterns compile");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(22), "{output:?}");
}

#[test]
fn multi_scrutinee_boolean_patterns_lower_and_prove_coverage() {
    let source = "module Main where\nchoose left right = case left, right of\n  true, true -> 11\n  true, false -> 12\n  false, true -> 13\n  false, false -> 14\nmain = choose false true\n";
    let artifact = compile_source("Main.purs", source).expect("Boolean product case compiles");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(13), "{output:?}");

    let partial = "module Main where\nmain = case true, false of\n  true, false -> 1\n";
    let errors = compile_source("Main.purs", partial)
        .expect_err("a Boolean product case missing rows remains partial");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("non-exhaustive case")),
        "{errors:?}"
    );
}

#[test]
fn nested_boolean_constructor_pattern_selects_the_matching_row() {
    let source = "module Main where\ndata Flag = Flag Boolean\nmain = case Flag true of\n  Flag true -> 11\n  _ -> 22\n";
    compile_source("Main.purs", source).expect("nested Boolean constructor patterns compile");
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(11), "{output:?}");
}

#[test]
fn integer_literal_function_equations_preserve_clause_order() {
    let source = "module Main where\nchoose 0 = 11\nchoose n\n  | n > 0 = 22\nchoose _ = 33\nmain = choose 0\n";
    let artifact = compile_source("Main.purs", source).expect("integer equations compile");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(11), "{output:?}");
}

#[test]
fn integer_literal_patterns_remain_in_typed_core() {
    let source = "module Main where\nmain = case 1 of\n  1 -> 10\n  _ -> 20\n";
    let core = lower_source_to_core("Main.purs", source).expect("integer patterns lower to Core");
    let main = core
        .declarations
        .iter()
        .find(|declaration| declaration.name == "main")
        .expect("Core should retain main");
    let psrs_core::ExprKind::Case { branches, .. } = &main.value.kind else {
        panic!("main should retain its case expression");
    };
    assert!(matches!(
        branches[0].pattern.kind,
        psrs_core::PatternKind::Literal {
            value: psrs_core::Literal::Integer(1)
        }
    ));
}

#[test]
fn integer_patterns_ignore_a_user_defined_equal_operator() {
    let source = "module Main where\ninfix 4 alwaysFalse as ==\nalwaysFalse _ _ = false\nmain = case 1 of\n  1 -> 10\n  _ -> 20\n";
    let artifact = compile_source("Main.purs", source).expect("custom equality pattern compiles");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(10),
        "literal patterns use compiler equality"
    );
}

#[test]
fn guarded_where_type_annotations_are_checked() {
    let source = "module Main where\nimport Prelude\nchoose n\n  | true = next\n  where\n    next :: Boolean\n    next = n\nmain = choose 10\n";
    let errors = compile_source("Main.purs", source)
        .expect_err("a local guarded-where annotation cannot be ignored");
    assert!(
        errors.iter().any(|error| {
            error.stage == "P5 typecheck" && error.code == Some("TypesDoNotUnify")
        }),
        "{errors:?}"
    );
}

#[test]
fn local_let_type_annotations_are_checked() {
    let source =
        "module Main where\nimport Prelude\nmain = let\n  next :: Boolean\n  next = 1\n in next\n";
    let errors = compile_source("Main.purs", source)
        .expect_err("a local let annotation must reach type checking");
    assert!(
        errors.iter().any(|error| {
            error.stage == "P5 typecheck" && error.code == Some("TypesDoNotUnify")
        }),
        "{errors:?}"
    );
}

#[test]
fn local_pattern_binding_is_visible_to_following_declarations() {
    let source =
        "module Main where\ndata Box a = Box a\nmain = let\n  Box value = Box 10\n in value\n";
    let artifact = compile_source("Main.purs", source).expect("local pattern binding compiles");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(10), "{output:?}");
}

#[test]
fn local_pattern_binding_does_not_scope_over_earlier_declarations() {
    let source = "module Main where\nimport Prelude\ndata X a = X a\nx =\n  let\n    b = a\n    X a = X 10\n  in b\n";
    let errors = compile_source("Main.purs", source).expect_err("a is declared later");
    assert!(
        errors
            .iter()
            .any(|error| { error.code == Some("UnknownName") && error.message.contains("`a`") }),
        "{errors:?}"
    );
}

#[test]
fn repeated_names_inside_a_case_named_pattern_are_rejected() {
    let source = "module Main where\nimport Prelude\ndata S a = S a (S a)\nf x = case x of\n  (S y (S y@(S z zs))) -> y\n";
    let errors = compile_source("Main.purs", source).expect_err("y is bound more than once");
    assert!(
        errors
            .iter()
            .any(|error| error.code == Some("OverlappingArgNames")),
        "{errors:?}"
    );
}

#[test]
fn multiple_case_scrutinees_preserve_order_and_guard_fallthrough() {
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = case runEffect (log \"1\"), runEffect (log \"2\"), runEffect (log \"3\"), runEffect (log \"4\"), runEffect (log \"5\"), runEffect (log \"6\"), runEffect (log \"7\"), runEffect (log \"8\"), runEffect (log \"9\"), runEffect (log \"10\") of\n  _, _, _, _, _, _, _, _, _, _ | false -> 10\n  _, _, _, _, _, _, _, _, _, _ | true -> 22\n";
    let artifact =
        compile_source("Main.purs", source).expect("multi-scrutinee guarded case compiles");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(22), "{output:?}");
    assert_eq!(output.stdout, b"1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n");
}

#[test]
fn later_guard_clause_binders_are_renamed_in_each_fallthrough_copy() {
    let source = "module Main where\nimport Prelude\nchoose n\n  | n > 0, n < 10 = 1\n  | let next = n + 1, next > 0 = next\n  | true = 0\nmain = choose 10\n";
    let artifact =
        compile_source("Main.purs", source).expect("guard continuations have fresh locals");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(11), "{output:?}");
}

#[test]
fn guarded_rows_do_not_claim_unconditional_coverage() {
    let source = "module Main where\nread n\n  | n > 0 = n\nmain = read 1\n";
    let errors = compile_source("Main.purs", source)
        .expect_err("a partial guarded equation is not exhaustive");
    assert_eq!(
        errors
            .iter()
            .filter(|error| error.message.contains("non-exhaustive case"))
            .count(),
        1,
        "{errors:?}"
    );

    let boolean_case =
        "module Main where\nmain = case true of\n  true | false -> 1\n  false -> 2\n";
    let errors = compile_source("Main.purs", boolean_case)
        .expect_err("a failing Boolean guard must not make its case exhaustive");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("non-exhaustive case"))
    );

    let exhaustive = "module Main where\nread n\n  | true = n\nmain = read 1\n";
    let artifact =
        compile_source("Main.purs", exhaustive).expect("an intrinsic true guard is exhaustive");
    assert!(!has_non_exhaustive_warning(&artifact));

    let typed_true = "module Main where\nread n\n  | true :: Boolean = n\nmain = read 1\n";
    let artifact = compile_source("Main.purs", typed_true)
        .expect("a typed Boolean true guard is unconditional");
    assert!(!has_non_exhaustive_warning(&artifact));
}

#[test]
fn official_guard_and_case_sources_lower_through_p2() {
    let corpus = std::env::var_os("PURESCRIPT_REPO").map_or_else(
        || std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream/passing"),
        |repo| std::path::PathBuf::from(repo).join("tests/purs/passing"),
    );
    let cases = [
        "2787.purs",
        "2806.purs",
        "DctorName.purs",
        "FunctionAndCaseGuards.purs",
        "TCO.purs",
        "2795.purs",
        "3114/VendoredVariant.purs",
        "4357.purs",
        "Guards.purs",
        "MultiArgFunctions.purs",
        "CaseMultipleExpressions.purs",
        "CaseInputWildcard.purs",
    ];
    let mut failures = Vec::new();
    for case in cases {
        let path = corpus.join(case);
        let source = std::fs::read_to_string(&path).expect("official fixture is readable");
        if let Err(errors) = lower_source_to_ast(&path.to_string_lossy(), &source) {
            failures.push(format!("{case}: {errors:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "official P2 lowering failures: {failures:#?}"
    );
}

#[test]
fn user_defined_false_otherwise_does_not_prove_guard_coverage() {
    let constants = "module Boolean.Constants (otherwise) where\nimport Prelude\notherwise :: Boolean\notherwise = false\n";
    let boolean = "module Data.Boolean (otherwise) where\nimport Boolean.Constants (otherwise)\n";
    let main = "module Main where\nimport Prelude\nimport Data.Boolean (otherwise)\nread n\n  | otherwise = n\nmain = read 1\n";
    let errors = compile_program_sources_with_prelude(&[
        ("Boolean.Constants.purs", constants),
        ("Data.Boolean.purs", boolean),
        ("Main.purs", main),
    ])
    .expect_err("an arbitrary binding named otherwise is not a coverage proof");
    assert_eq!(
        errors
            .iter()
            .filter(|error| error.diagnostic.message.contains("non-exhaustive case"))
            .count(),
        1,
        "{errors:?}"
    );
}

#[test]
fn cross_module_true_alias_is_a_verified_unconditional_guard() {
    let constants =
        "module Boolean.Constants (truth) where\nimport Prelude\ntruth :: Boolean\ntruth = true\n";
    let boolean = "module Data.Boolean (otherwise) where\nimport Prelude\nimport Boolean.Constants (truth)\notherwise :: Boolean\notherwise = truth\n";
    let main = "module Main where\nimport Prelude\nimport Data.Boolean (otherwise)\nread n\n  | otherwise = n\nmain = read 11\n";
    let artifact = compile_program_sources_with_prelude(&[
        ("Boolean.Constants.purs", constants),
        ("Data.Boolean.purs", boolean),
        ("Main.purs", main),
    ])
    .expect("resolved aliases to true prove guard coverage");
    assert!(
        artifact
            .warnings
            .iter()
            .all(|warning| !warning.diagnostic.message.contains("non-exhaustive case")),
        "{:?}",
        artifact.warnings
    );
}

pub(super) fn has_non_exhaustive_warning(artifact: &crate::Artifact) -> bool {
    artifact
        .warnings
        .iter()
        .any(|warning| warning.diagnostic.message.contains("non-exhaustive case"))
}
