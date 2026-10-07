use super::*;

#[test]
fn shared_import_aliases_match_official_purs() {
    if !purs_available() {
        eprintln!("skipping: purs is not installed");
        return;
    }
    let a = "module A where\ndata Item = Item\nthing :: Item\nthing = Item\n";
    let disjoint = "module B where\ndata Other = Other\nother :: Other\nother = Other\n";
    let canonical = "module B (module A) where\nimport A\n";
    let conflicting = "module B where\ndata Item = Other\nthing :: Item\nthing = Other\n";
    for (name, b, main, expected) in [
        (
            "disjoint-reexport",
            disjoint,
            "module Main (module X, value) where\nimport A as X\nimport B as X\nvalue :: X.Other\nvalue = X.other\n",
            true,
        ),
        (
            "canonical-reexport",
            canonical,
            "module Main (module X, value) where\nimport A as X\nimport B as X\nvalue :: X.Item\nvalue = X.thing\n",
            true,
        ),
        (
            "unused-conflict",
            conflicting,
            "module Main where\nimport A as X\nimport B as X\nmain :: Int\nmain = 42\n",
            true,
        ),
        (
            "value-conflict",
            conflicting,
            "module Main where\nimport A as X\nimport B as X\nmain = X.thing\n",
            false,
        ),
        (
            "type-conflict",
            conflicting,
            "module Main where\nimport A as X\nimport B as X\nmain :: X.Item\nmain = X.Item\n",
            false,
        ),
        (
            "reexport-conflict",
            conflicting,
            "module Main (module X) where\nimport A as X\nimport B as X\n",
            false,
        ),
    ] {
        let sources = [("A.purs", a), ("B.purs", b), ("Main.purs", main)];
        let purs = purs_sources_output(&format!("import-alias-{name}"), &sources);
        assert_eq!(purs.status.success(), expected, "{name}: {purs:?}");
        let ours = psrs_driver::resolve_program_sources(&sources);
        assert_eq!(ours.is_ok(), expected, "{name}: {ours:?}");
        if !expected {
            assert!(
                purs_error_codes(&purs)
                    .iter()
                    .any(|code| code == "ScopeConflict"),
                "{name}: {purs:?}"
            );
            assert!(
                ours.unwrap_err()
                    .iter()
                    .any(|error| error.diagnostic.code == Some("ScopeConflict")),
                "{name}"
            );
        }
    }
}
