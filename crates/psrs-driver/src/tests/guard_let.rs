use super::guards::has_non_exhaustive_warning;
use super::*;

#[test]
fn ordinary_let_in_expression_is_supported_as_a_boolean_guard() {
    let source = "module Main where\nimport Prelude\npositive n\n  | (let next = n + 1 in next > 0), n < 20 = n + 1\n  | true = 0\nmain = positive 9\n";
    let artifact = compile_source("Main.purs", source).expect("let-in expression guard compiles");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(10), "{output:?}");
}

#[test]
fn ordinary_let_in_guard_binders_do_not_escape_the_expression() {
    let sources = [
        "module Main where\nimport Prelude\nchoose n\n  | (let scoped = n in true), scoped > 0 = n\n  | true = 0\nmain = choose 9\n",
        "module Main where\nimport Prelude\nchoose n\n  | (let scoped = n in true) = scoped\n  | true = 0\nmain = choose 9\n",
    ];
    for source in sources {
        let errors = compile_source("Main.purs", source)
            .expect_err("an expression-local let binder cannot escape to a later guard or body");
        assert!(
            errors
                .iter()
                .any(|error| { error.stage == "P3 resolve" && error.code == Some("UnknownName") }),
            "expected a name-resolution error for the escaped local: {errors:?}"
        );
    }
}

#[test]
fn issue_requested_naked_let_guard_binds_values_for_later_guards_and_the_body() {
    // This cross-guard binding qualifier is an issue-specific extension. Official PureScript
    // supports let-in expressions inside Boolean guards, not this naked qualifier form.
    let source = "module Main where\npositive n\n  | let next = n + 1, next > 0 = next\n  | true = 0\nmain = positive 9\n";
    let artifact = compile_source("Main.purs", source).expect("naked let guard extension compiles");
    assert!(!has_non_exhaustive_warning(&artifact));
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(10), "{output:?}");
}
