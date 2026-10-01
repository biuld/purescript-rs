use super::*;

#[path = "../support/rank_n/mod.rs"]
mod cases;

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
