use super::*;

#[test]
fn guarded_polymorphic_results_match_official_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    for (name, constraint, body, accepted) in [
        (
            "single-row",
            "Build f => ",
            "choose = case _, _ of\n  flag, value\n    | flag -> build value\n    | true -> build value\n",
            true,
        ),
        (
            "later-guard",
            "Build f => ",
            "choose flag value = case flag of\n  false -> build value\n  true | flag -> build value\n  true -> build value\n",
            true,
        ),
        (
            "missing-dictionary",
            "",
            "choose = case _, _ of\n  flag, value\n    | flag -> build value\n    | true -> build value\n",
            false,
        ),
    ] {
        let source = format!(
            "module Main where\nclass Build f where\n  build :: forall a. a -> f a\nchoose :: forall f a. {constraint}Boolean -> a -> f a\n{body}main :: Int\nmain = 42\n"
        );
        let sources = [("Main.purs", source.as_str())];
        let purs = purs_sources_output(&format!("guard-result-{name}"), &sources);
        assert_eq!(purs.status.success(), accepted, "{name}: {purs:?}");
        let ours = psrs_driver::check_program_types_lenient(&sources);
        assert_eq!(ours.is_ok(), accepted, "{name}: {ours:?}");
        if !accepted {
            assert!(
                purs_error_codes(&purs)
                    .iter()
                    .any(|code| code == "NoInstanceFound"),
                "{name}: {purs:?}"
            );
            assert!(
                ours.unwrap_err()
                    .iter()
                    .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
                "{name}"
            );
        }
    }
}
