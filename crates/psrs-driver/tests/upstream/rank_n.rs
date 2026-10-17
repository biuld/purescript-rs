use super::*;

#[path = "../support/rank_n/mod.rs"]
mod cases;

#[test]
fn differential_contextual_application_result_diagnostic() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    for (name, source) in [
        ("contextual-rigid-result", cases::RIGID_RESULT),
        ("contextual-nominal-result", cases::RIGID_NOMINAL_RESULT),
    ] {
        let official = purs_sources_output(name, &[("Main.purs", source)]);
        assert!(!official.status.success(), "{official:?}");
        assert!(
            purs_error_codes(&official).contains(&"TypesDoNotUnify".to_owned()),
            "{official:?}"
        );
        let errors = psrs_driver::check_source("Main.purs", source)
            .expect_err("the quantified argument cannot satisfy the concrete result");
        assert!(
            errors
                .iter()
                .any(|error| error.code == Some("TypesDoNotUnify")),
            "{errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| error.code == Some("EscapedSkolem")),
            "{errors:?}"
        );
    }
}

#[test]
fn differential_rank_n_rules_against_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    assert!(purs_accepts_sources("rank-n-linked", cases::LINKED));
    psrs_driver::check_program(cases::LINKED).expect("check imported quantified types");
    for (expected, cases) in [
        (true, cases::ACCEPT),
        (true, cases::CHECK_ONLY),
        (false, cases::REJECT),
    ] {
        for (name, source) in cases {
            assert_eq!(
                purs_accepts_sources(&format!("rank-n-{name}"), &[("Main.purs", source)]),
                expected,
                "{name}: official compiler disagrees with the fixture"
            );
            let result = psrs_driver::check_source(name, source);
            assert_eq!(result.is_ok(), expected, "{name}: {result:?}");
        }
    }
}
