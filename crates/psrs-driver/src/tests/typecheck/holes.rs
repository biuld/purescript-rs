//! Typed holes on the shipped type checker.
//!
//! A hole is an inference variable. The diagnostic reports the type that
//! variable was solved to, under the official `HoleInferredType` code.

use super::*;

fn hole_messages(errors: &[ProgramDiagnostic]) -> Vec<&str> {
    errors
        .iter()
        .filter(|error| error.diagnostic.code == Some("HoleInferredType"))
        .map(|error| error.diagnostic.message.as_str())
        .collect()
}

#[test]
fn a_hole_checked_against_int_reports_int() {
    let source = "module Main where\nanswer :: Int\nanswer = ?hole\n";
    let errors = check_program_types_lenient(&[("Main.purs", source)])
        .expect_err("a typed hole is rejected");
    let messages = hole_messages(&errors);
    assert!(
        messages.iter().any(|message| message.contains("Int")),
        "{errors:?}"
    );
}

#[test]
fn a_type_hole_reports_the_solved_argument() {
    let source = "module Main where\nvalue :: ?hole\nvalue = 1\n";
    let errors =
        check_program_types_lenient(&[("Main.purs", source)]).expect_err("a type hole is rejected");
    let messages = hole_messages(&errors);
    assert!(
        messages.iter().any(|message| message.contains("Int")),
        "{errors:?}"
    );
}

#[test]
fn an_expression_hole_in_effect_reports_the_solved_type() {
    let source = include_str!("../../../../../tests/upstream/failing/TypedHole.purs");
    let errors = check_program_types_lenient_with_prelude(&[("TypedHole.purs", source)])
        .expect_err("a typed hole is rejected");
    let messages = hole_messages(&errors);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Effect") && message.contains("Unit")),
        "{errors:?}"
    );
}

#[test]
fn a_type_hole_in_effect_reports_the_solved_argument() {
    let source = include_str!("../../../../../tests/upstream/failing/TypedHole2.purs");
    let errors = check_program_types_lenient_with_prelude(&[("TypedHole2.purs", source)])
        .expect_err("a type hole is rejected");
    let messages = hole_messages(&errors);
    assert!(
        messages.iter().any(|message| message.contains("Unit")),
        "{errors:?}"
    );
}

#[test]
fn an_unannotated_hole_is_still_reported() {
    let source = include_str!("../../../../../tests/upstream/failing/TypedHole3.purs");
    let errors = check_program_types_lenient(&[("TypedHole3.purs", source)])
        .expect_err("a typed hole is rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("HoleInferredType")),
        "{errors:?}"
    );
}

#[test]
fn a_hole_applied_to_an_argument_reports_the_solved_function_type() {
    let source = include_str!("../../../../../tests/upstream/failing/4408Acyclic.purs");
    let errors = check_program_types_lenient(&[("4408Acyclic.purs", source)])
        .expect_err("a typed hole is rejected");
    let messages = hole_messages(&errors);
    assert!(
        messages.iter().any(|message| message.contains("Int")),
        "{errors:?}"
    );
}
