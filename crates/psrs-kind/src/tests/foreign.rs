use super::*;

#[test]
fn foreign_value_annotations_are_checked_at_their_declaring_module() {
    let modules = resolve_program(&[
        (
            "Main.purs",
            "module Main where\nimport Lib (bad)\nmain = bad\n",
        ),
        (
            "Lib.purs",
            "module Lib where\ndata Region\nforeign import data R :: Region\nforeign import bad :: R\n",
        ),
        (
            "Other.purs",
            "module Other where\nimport Lib (bad)\nvalue = bad\n",
        ),
    ]);
    let diagnostics = check_modules(&modules);
    // A foreign value's annotation kind equation fails through the official
    // `checkTypeKind`, which reports `ExpectedType` at the annotation.
    let conflicts = diagnostics
        .iter()
        .filter(|error| error.code == "ExpectedType")
        .collect::<Vec<_>>();
    assert_eq!(conflicts.len(), 1, "{diagnostics:?}");
    assert_eq!(conflicts[0].origin, ModuleId(1));
}

#[test]
fn foreign_annotations_preserve_parameter_kinds_and_forall_scope() {
    for source in [
        "module Test where\ndata Region\ndata Box :: Region -> Type\ndata Box r = Box\nforeign import bad :: Box Int\n",
        "module Test where\nforeign import bad :: a\n",
    ] {
        let diagnostics = check(source);
        assert!(
            diagnostics
                .iter()
                .any(|error| matches!(error.code, "KindsDoNotUnify" | "UndefinedTypeVariable")),
            "{diagnostics:?}"
        );
    }
    assert!(check("module Test where\nforeign import good :: forall a. a -> a\n").is_empty());
}
