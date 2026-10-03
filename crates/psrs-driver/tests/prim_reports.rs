use psrs_driver::{DiagnosticOrigin, check_program_with_warnings, check_source};

const FAIL_DOC: &str = include_str!("fixtures/prim/reports/fail_beside_above.purs");
const FAIL_MULTILINE_BESIDE: &str =
    include_str!("fixtures/prim/reports/fail_multiline_beside.purs");
const FAIL_RESIDUAL: &str = include_str!("fixtures/prim/reports/fail_residual.purs");
const PARTIAL_RESIDUAL: &str = include_str!("fixtures/prim/reports/partial_residual.purs");

fn rejection(source: &str) -> Vec<psrs_driver::Diagnostic> {
    check_source("Main.purs", source).expect_err("the report constraint must reject")
}

#[test]
fn fail_renders_the_structured_doc_tree() {
    let errors = rejection(FAIL_DOC);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, Some("NoInstanceFound"));
    assert!(
        errors[0]
            .message
            .contains("Custom error:\n  bad \"h e l l o\"\n  value"),
        "{}",
        errors[0].message
    );
}

#[test]
fn fail_beside_pads_multiline_left_documents_by_their_widest_line() {
    let errors = rejection(FAIL_MULTILINE_BESIDE);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, Some("NoInstanceFound"));
    assert!(
        errors[0].message.contains("Custom error:\n  long!\n  x"),
        "{}",
        errors[0].message
    );
}

#[test]
fn fail_propagates_through_an_instance_context_without_a_second_error() {
    let errors = rejection(FAIL_RESIDUAL);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, Some("NoInstanceFound"));
    assert!(errors[0].message.contains("Custom error:"), "{errors:?}");
    assert!(errors[0].message.contains("custom residual"), "{errors:?}");
}

#[test]
fn partial_residual_is_reported_at_its_concrete_use() {
    let errors = rejection(PARTIAL_RESIDUAL);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, Some("NoInstanceFound"));
    assert!(errors[0].message.contains("Partial"), "{errors:?}");
    let use_site = PARTIAL_RESIDUAL.rfind("foo").expect("concrete use") as u32;
    assert!(
        (errors[0].span.start..errors[0].span.end).contains(&use_site),
        "diagnostic span {:?} should point at the concrete Partial use {use_site}",
        errors[0].span
    );
}

#[test]
fn constrained_forall_ascription_preserves_expected_polymorphism_and_evidence() {
    let source = "\
module Main where
class Identity a where
  identity :: a -> a
instance identityInt :: Identity Int where
  identity value = value
poly :: forall a. Identity a => a -> a
poly value = identity value
applyPoly :: (forall a. Identity a => a -> a) -> Int
applyPoly function = function 42
mono :: Int -> Int
mono = (poly :: forall a. Identity a => a -> a)
main :: Int
main = applyPoly (poly :: forall a. Identity a => a -> a)
";
    check_source("Main.purs", source)
        .expect("constrained-forall evidence must survive monomorphic and rank-N uses");
}

#[test]
fn constrained_forall_warn_ascription_reports_when_used_monomorphically() {
    let source = "\
module Main where
import Prim.TypeError (class Warn, Text)
identity :: forall a. Warn (Text \"polymorphic warning\") => a -> a
identity value = value
mono :: Int -> Int
mono = (identity :: forall a. Warn (Text \"polymorphic warning\") => a -> a)
";
    let warnings = psrs_driver::check_source_with_warnings("Main.purs", source)
        .expect("Warn should discharge the constrained ascription");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].diagnostic.code, Some("UserDefinedWarning"));
    assert!(
        warnings[0]
            .diagnostic
            .message
            .contains("polymorphic warning")
    );
}

#[test]
fn constrained_forall_fail_ascription_reports_at_its_monomorphic_use() {
    let source = "\
module Main where
import Prim.TypeError (class Fail, Text)
identity :: forall a. Fail (Text \"polymorphic failure\") => a -> a
identity value = value
mono :: Int -> Int
mono = (identity :: forall a. Fail (Text \"polymorphic failure\") => a -> a)
";
    let errors = rejection(source);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, Some("NoInstanceFound"));
    assert!(errors[0].message.contains("Custom error:"), "{errors:?}");
    assert!(
        errors[0].message.contains("polymorphic failure"),
        "{errors:?}"
    );
}

#[test]
fn warn_prefers_a_given_and_attributes_imported_warning_to_the_caller() {
    let library = "\
module Lib where
import Prim.TypeError (class Warn, Text)
foo :: Warn (Text \"foo\") => Int -> Int
foo value = value
";
    let main = "\
module Main where
import Prim.TypeError (class Warn, Text)
import Lib (foo)
given :: Warn (Text \"foo\") => Int
given = foo 1
trigger :: Int
trigger = foo 2
";
    let warnings = check_program_with_warnings(&[("Lib.purs", library), ("Main.purs", main)])
        .expect("Warn constraints report warnings without rejecting the program");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let warning = &warnings[0];
    assert_eq!(warning.source, DiagnosticOrigin::Source(1));
    assert_eq!(warning.diagnostic.code, Some("UserDefinedWarning"));
    assert!(warning.diagnostic.message.contains("foo"), "{warning:?}");
    let trigger = main
        .find("trigger :: Int")
        .expect("owning declaration signature is present") as u32;
    assert!(
        (warning.diagnostic.span.start..warning.diagnostic.span.end).contains(&trigger),
        "warning span {:?} should include caller declaration at {trigger}",
        warning.diagnostic.span
    );
}

#[test]
fn inferred_warn_is_emitted_once_and_not_retained_in_the_inferred_scheme() {
    let source = include_str!("fixtures/prim/reports/warn_inferred_no_residual.purs");
    let warnings = psrs_driver::check_source_with_warnings("Main.purs", source)
        .expect("Warn should report and discharge in the inferred declaration");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warnings[0].diagnostic.code, Some("UserDefinedWarning"));
    let inferred = source
        .find("inferred value")
        .expect("warning-owning inferred declaration") as u32;
    assert!(
        (warnings[0].diagnostic.span.start..warnings[0].diagnostic.span.end).contains(&inferred),
        "warning span {:?} should identify the inferred declaration at {inferred}",
        warnings[0].diagnostic.span
    );
}
