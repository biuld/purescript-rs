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
fn let_guards_bind_values_for_later_guards_and_the_body() {
    let source = "module Main where\npositive n\n  | let next = n + 1, next > 0 = next\n  | true = 0\nmain = positive 9\n";
    let artifact = compile_source("Main.purs", source).expect("let guards compile");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(10), "{output:?}");
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
    let source = "module Main where\nimport Prelude\nimport WASI.Console\nmain = let left = runEffect (log \"left\") in\n  let right = runEffect (log \"right\") in\n    case left, right of\n      _, _ | false -> 10\n      _, _ | true -> 22\n";
    let artifact =
        compile_source("Main.purs", source).expect("multi-scrutinee guarded case compiles");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(22), "{output:?}");
    assert_eq!(output.stdout, b"left\nright\n");
}

#[test]
fn guarded_rows_do_not_claim_unconditional_coverage() {
    let source = "module Main where\nread n\n  | n > 0 = n\nmain = read 1\n";
    let errors = compile_source("Main.purs", source)
        .expect_err("a partial guarded equation is not exhaustive");
    assert!(
        errors
            .iter()
            .any(|error| { error.message.contains("non-exhaustive case") })
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
        compile_source("Main.purs", exhaustive).expect("otherwise makes the row exhaustive");
    assert!(!has_non_exhaustive_warning(&artifact));
}

fn has_non_exhaustive_warning(artifact: &crate::Artifact) -> bool {
    artifact
        .warnings
        .iter()
        .any(|warning| warning.diagnostic.message.contains("non-exhaustive case"))
}
