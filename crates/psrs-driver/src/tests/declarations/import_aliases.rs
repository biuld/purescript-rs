use super::*;

#[test]
fn a_shared_qualifier_combines_disjoint_values_and_types() {
    let a = "module A where\ndata Left = Left\na :: Left\na = Left\n";
    let b = "module B where\ndata Right = Right\nb :: Right\nb = Right\n";
    let facade = "module Facade (module X, left, right) where\nimport A as X\nimport B as X\nleft :: X.Left\nleft = X.a\nright :: X.Right\nright = X.Right\n";
    let main = "module Main where\nimport Facade (Left(..), Right(..), a, b)\nleft :: Left\nleft = a\nright :: Right\nright = b\n";
    resolve_program_sources(&[
        ("A.purs", a),
        ("B.purs", b),
        ("Facade.purs", facade),
        ("Main.purs", main),
    ])
    .unwrap();
}

#[test]
fn shared_qualifiers_preserve_identity_through_facades() {
    let a = "module A where\ndata Thing = Thing\nthing :: Thing\nthing = Thing\n";
    let b = "module B (module A) where\nimport A\n";
    let main = "module Main (module X, value) where\nimport A as X\nimport B as X\nvalue :: X.Thing\nvalue = X.thing\n";
    let modules =
        resolve_program_sources(&[("A.purs", a), ("B.purs", b), ("Main.purs", main)]).unwrap();
    let exports = modules[2].exports.as_ref().unwrap();
    assert_eq!(exports.types.len(), 1);
    assert_eq!(
        exports.types[0].reference,
        psrs_hir::TypeReference::Named(modules[0].types[0].id)
    );
    assert_eq!(
        exports
            .values
            .iter()
            .filter(|value| value.name == "thing")
            .count(),
        1
    );
}

#[test]
fn an_unused_shared_qualifier_does_not_create_a_conflict() {
    let a = "module A where\nthing = 1\n";
    let b = "module B where\nthing = 2\n";
    let main = "module Main where\nimport A as X\nimport B as X\nmain = 42\n";
    resolve_program_sources(&[("A.purs", a), ("B.purs", b), ("Main.purs", main)]).unwrap();
}

#[test]
fn shared_qualifier_conflicts_are_reported_at_use_or_reexport() {
    for main in [
        "module Main where\nimport A as X\nimport B as X\nmain = X.thing\n",
        "module Main (module X) where\nimport A as X\nimport B as X\n",
        "module Main where\nimport A as X\nimport B as X\nvalue :: X.Thing\nvalue = X.First\n",
    ] {
        let a = "module A where\ndata Thing = First\nthing = 1\n";
        let b = "module B where\ndata Thing = Second\nthing = 2\n";
        let errors = resolve_program_sources(&[("A.purs", a), ("B.purs", b), ("Main.purs", main)])
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.code == Some("ScopeConflict")),
            "{errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| error.diagnostic.code == Some("ExportConflict")),
            "{errors:?}"
        );
    }
}

#[test]
fn the_official_list_exports_resolve_with_the_locked_stdlib() {
    let main =
        "module Main where\nimport Data.List as L\nmain = L.length (L.fromFoldable [1,2,3])\n";
    check_program_lenient_with_prelude(&[("Main.purs", main)]).unwrap();
}
