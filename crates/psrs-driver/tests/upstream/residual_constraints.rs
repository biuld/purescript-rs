use super::{purs_available, purs_error_codes, purs_sources_output};
use psrs_driver::check_source;

#[test]
fn local_binding_constraints_and_annotations_match_official_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    for (name, body, expected) in [
        (
            "unused-constrained-local",
            "class C a where\n  method :: a -> a\nf y = let g x = method x in y\n",
            Some("AmbiguousTypeVariables"),
        ),
        (
            "used-constrained-local",
            "class C a where\n  method :: a -> a\nf y = let g x = method x in g y\n",
            None,
        ),
        (
            "unannotated-local",
            "f = let ident x = x in { a: ident 1, b: ident true }\n",
            Some("TypesDoNotUnify"),
        ),
        (
            "annotated-local",
            "f = let ident = (\\x -> x) :: forall a. a -> a in { a: ident 1, b: ident true }\n",
            None,
        ),
        (
            "annotated-constrained-local",
            "class C a where\n  method :: a -> a\nf y = let g = (\\x -> method x) :: forall a. C a => a -> a in y\n",
            None,
        ),
        (
            "fundep-determined-local",
            "class C a b | a -> b where\n  method :: a -> b\nf x = let g y = method y in g x\n",
            None,
        ),
    ] {
        let source = format!("module Main where\n{body}");
        let official = purs_sources_output(name, &[("Main.purs", &source)]);
        let local = check_source("Main.purs", &source);
        assert_eq!(
            official.status.success(),
            expected.is_none(),
            "{name}: {official:?}"
        );
        match expected {
            Some(code) => {
                assert!(
                    purs_error_codes(&official)
                        .iter()
                        .any(|found| found == code),
                    "{name}: {official:?}"
                );
                let errors = local.expect_err(name);
                assert!(
                    errors.iter().any(|error| error.code == Some(code)),
                    "{name}: {errors:?}"
                );
            }
            None => local.unwrap_or_else(|errors| panic!("{name}: {errors:?}")),
        }
    }
}

#[test]
fn official_constraint_inference_case_reports_its_annotated_ambiguity() {
    let source = include_str!("../../../../tests/upstream/failing/ConstraintInference.purs");
    assert!(source.contains("@shouldFailWith AmbiguousTypeVariables"));
    let errors = check_source("ConstraintInference.purs", source)
        .expect_err("the inferred result does not determine Show's argument");
    assert!(
        errors
            .iter()
            .any(|error| error.code == Some("AmbiguousTypeVariables")),
        "{errors:?}"
    );
}
