use super::*;

#[test]
fn saturated_aliases_apply_or_discard_their_partial_alias_arguments() {
    for usage in ["Apply (Wrap Int) Boolean", "Ignore (Wrap Int)"] {
        check_ok(&format!(
            "module Main where\ntype Apply f x = f x\ntype Ignore f = Int\ntype Wrap a b = {{ first :: a, second :: b }}\nvalue :: {usage}\nvalue = value\n"
        ));
    }
}

#[test]
fn applying_an_open_row_alias_normalizes_before_saturation() {
    check_ok(
        "module Main where\ntype Apply f x = f x\ntype STATE s r = (state :: Array s | r)\nvalue :: Record (Apply (STATE Int) ())\nvalue = value\n",
    );
}

#[test]
fn repeated_higher_order_alias_application_is_not_a_declaration_cycle() {
    check_ok(
        "module Main where\ntype Apply f x = f x\ntype Wrap a b = { first :: a, second :: b }\nvalue :: Apply (Apply (Wrap Int)) Boolean\nvalue = value\n",
    );
}

#[test]
fn a_remaining_partial_alias_under_a_data_constructor_is_rejected() {
    let errors = check(
        "module Main where\ndata Holder f = Holder (f Boolean)\ntype Wrap a b = { first :: a, second :: b }\nvalue :: Holder (Wrap Int)\nvalue = value\n",
    );
    assert!(
        codes(&errors).contains(&"PartiallyAppliedSynonym"),
        "{errors:?}"
    );
}

#[test]
fn module_rechecks_retain_imported_alias_templates_and_arity() {
    let modules = resolve_program(&[
        (
            "Alias.purs",
            "module Alias where\ntype Apply f x = f x\ntype Wrap a b = { first :: a, second :: b }\n",
        ),
        (
            "Main.purs",
            "module Main where\nimport Alias (Apply, Wrap)\nvalue :: Apply (Wrap Int) Boolean\nvalue = value\n",
        ),
    ]);
    let (environment, errors) = check_env(&modules);
    assert!(errors.is_empty(), "{errors:?}");
    let errors = crate::check_module(&modules[1], &environment);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn discarded_alias_arguments_still_satisfy_declared_kinds() {
    let errors = check(
        "module Main where\ntype Ignore :: Type -> Type\ntype Ignore a = Int\nvalue :: Ignore Array\nvalue = value\n",
    );
    assert!(codes(&errors).contains(&"KindsDoNotUnify"), "{errors:?}");
}
