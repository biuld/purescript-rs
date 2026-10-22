//! Constructor-pattern arity on the shipped type checker.
//!
//! `purs` reports `IncorrectConstructorArity` from `inferBinder` when a
//! constructor pattern's argument count differs from the constructor type.
//! These tests drive the lenient type check, with the standard library, on
//! the corpus files. That is the L4 entry.

use super::*;

fn codes(source: &str) -> Vec<&'static str> {
    check_program_types_lenient_with_prelude(&[("Main.purs", source)])
        .err()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|error| error.diagnostic.code)
        .collect()
}

#[test]
fn a_nested_constructor_pattern_reports_incorrect_constructor_arity() {
    let source = include_str!("../../../../../tests/upstream/failing/1169.purs");
    let codes = codes(source);
    assert!(codes.contains(&"IncorrectConstructorArity"), "{codes:?}");
    assert!(
        !codes.contains(&"TypesDoNotUnify"),
        "arity is its own code, not a type mismatch: {codes:?}"
    );
}

#[test]
fn a_case_pattern_reports_incorrect_constructor_arity() {
    let source = include_str!(
        "../../../../../tests/upstream/failing/CaseDoesNotMatchAllConstructorArgs.purs"
    );
    let codes = codes(source);
    assert!(codes.contains(&"IncorrectConstructorArity"), "{codes:?}");
}

#[test]
fn an_infix_let_pattern_reports_incorrect_constructor_arity() {
    let source = include_str!("../../../../../tests/upstream/failing/LetPatterns3.purs");
    let codes = codes(source);
    assert!(codes.contains(&"IncorrectConstructorArity"), "{codes:?}");
}
