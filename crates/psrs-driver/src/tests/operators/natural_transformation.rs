use super::*;

#[test]
fn unimported_natural_transformation_alias_is_rejected_in_bare_and_infix_forms() {
    for ty in ["Holder ((~>) Int)", "Int ~> Int"] {
        let source = format!(
            "module Main where\ndata Holder f = Holder (f Int)\nfoo :: {ty}\nfoo = foo\nmain = 42\n"
        );
        let errors = resolve_program_sources(&[("Main.purs", &source)])
            .expect_err("a type operator requires a declaration or import");
        assert!(
            errors
                .iter()
                .any(|error| error.diagnostic.code == Some("UnknownName")),
            "{errors:?}"
        );
    }
}

#[test]
fn hiding_the_natural_transformation_alias_does_not_restore_a_builtin_arrow() {
    let library = (
        "Alias.purs",
        "module Alias where\ninfixr 0 type Function as ~>\n",
    );
    let source = "module Main where\nimport Alias hiding (type (~>))\ndata Holder f = Holder (f Int)\nfoo :: Holder ((~>) Int)\nfoo = Holder (\\x -> x)\nmain = 42\n";
    let errors = resolve_program_sources(&[library, ("Main.purs", source)])
        .expect_err("a hidden type alias is not in scope");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("UnknownName")),
        "{errors:?}"
    );
}

#[test]
fn a_declared_bare_natural_transformation_alias_retains_its_target() {
    let source = "module Main where\ninfixr 0 type Function as ~>\ndata Holder f = Holder (f Int)\nfoo :: Holder ((~>) Int)\nfoo = Holder (\\x -> x)\nmain = 42\n";
    typecheck_program_sources(&[("Main.purs", source)])
        .expect("the explicitly declared Function alias is a valid partial constructor");
}

#[test]
fn other_bare_type_operator_aliases_use_the_same_lookup() {
    let source = "module Main where\ninfixr 0 type Function as :+\ndata Holder f = Holder (f Int)\nfoo :: Holder ((:+) Int)\nfoo = Holder (\\x -> x)\nmain = 42\n";
    typecheck_program_sources(&[("Main.purs", source)]).expect("ordinary operator alias");
    let source = source.replace("infixr 0 type Function as :+\n", "");
    let errors = resolve_program_sources(&[("Main.purs", &source)]).expect_err("missing alias");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("UnknownName")),
        "{errors:?}"
    );
}
