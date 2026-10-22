//! Anonymous arguments that are not section holes.
//!
//! `purs` reports `IncorrectAnonymousArgument` from
//! `Sugar.Operators.removeBinaryNoParens` when `_` is not an immediate operand
//! of a parenthesized operator. These tests drive the lenient resolver, which
//! is the L2 entry.

use super::*;

fn codes(source: &str) -> Vec<&'static str> {
    check_program_lenient_with_prelude(&[("Main.purs", source)])
        .err()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|error| error.diagnostic.code)
        .collect()
}

fn assert_anonymous(source: &str) {
    let codes = codes(source);
    assert!(codes.contains(&"IncorrectAnonymousArgument"), "{codes:?}");
    assert!(
        !codes.contains(&"UnknownName"),
        "a leftover `_` is not a value name: {codes:?}"
    );
}

#[test]
fn a_bare_underscore_is_an_incorrect_anonymous_argument() {
    assert_anonymous(include_str!(
        "../../../../../tests/upstream/failing/AnonArgument1.purs"
    ));
}

#[test]
fn an_unparenthesized_operator_hole_is_an_incorrect_anonymous_argument() {
    assert_anonymous(include_str!(
        "../../../../../tests/upstream/failing/AnonArgument2.purs"
    ));
    assert_anonymous(include_str!(
        "../../../../../tests/upstream/failing/AnonArgument3.purs"
    ));
}

#[test]
fn a_hole_inside_a_larger_section_is_an_incorrect_anonymous_argument() {
    assert_anonymous(include_str!(
        "../../../../../tests/upstream/failing/OperatorSections2.purs"
    ));
}

#[test]
fn an_immediate_section_hole_is_not_an_anonymous_argument_error() {
    let source = "module Main where\nimport Prelude\nmain = (_ + 2 * 3) 1\n";
    let result = check_program_lenient_with_prelude(&[("Main.purs", source)]);
    let codes = result
        .as_ref()
        .err()
        .map(|errors| {
            errors
                .iter()
                .filter_map(|error| error.diagnostic.code)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(!codes.contains(&"IncorrectAnonymousArgument"), "{codes:?}");
    assert!(result.is_ok(), "{codes:?}");
}
