//! A kind unknown left in a constructor field or a synonym body is
//! `QuantificationCheckFailureInType`.

use super::{check_env, check_ok, codes, resolve_program};

const FAILURE: &str =
    include_str!("../../../../tests/upstream/failing/QuantificationCheckFailure2.purs");

#[test]
fn an_unsolved_kind_inside_a_constructor_forall_is_rejected() {
    let modules = resolve_program(&[("Main.purs", FAILURE)]);
    let (_, diagnostics) = check_env(&modules);
    assert!(
        codes(&diagnostics).contains(&"QuantificationCheckFailureInType"),
        "{diagnostics:?}"
    );
}

#[test]
fn a_rank_n_field_whose_kinds_are_determined_is_accepted() {
    let source = "\
module Main where
data Proxy a = Proxy
data Rank = Rank (forall a. a -> a)
data Concrete = Concrete (Proxy Int)
data Poly a = Poly (Proxy a)
data Annotated = Annotated (forall (a :: Type). Proxy a)
";
    let modules = resolve_program(&[("Main.purs", source)]);
    let (environment, diagnostics) = check_env(&modules);
    assert!(
        !codes(&diagnostics).contains(&"QuantificationCheckFailureInType"),
        "{diagnostics:?}"
    );
    let proxy = modules[0]
        .types
        .iter()
        .find(|declaration| declaration.name == "Proxy")
        .expect("Proxy");
    let scheme = environment.kind_scheme(proxy.id).expect("Proxy's kind");
    assert!(
        !scheme.variables.is_empty(),
        "Proxy stays polykinded: {scheme:?}"
    );
}

#[test]
fn a_monomorphic_binding_group_rejects_a_mismatched_argument() {
    let source = include_str!("../../../../tests/upstream/failing/MonoKindDataBindingGroup.purs");
    let modules = resolve_program(&[("Main.purs", source)]);
    let (_, diagnostics) = check_env(&modules);
    let found = codes(&diagnostics);
    assert!(
        found.contains(&"KindsDoNotUnify"),
        "A and B share a parameter kind, so A \"bad\" is Symbol vs Type: {diagnostics:?}"
    );
    assert!(
        !found.contains(&"QuantificationCheckFailureInType"),
        "{diagnostics:?}"
    );
}

#[test]
fn a_use_written_before_its_binding_group_still_rejects_a_bad_kind() {
    let source = "\
module Main where
type X = A \"bad\"
data F (a :: Type -> Type) = F
data A a = A (B a)
type B a = F A
";
    let modules = resolve_program(&[("Main.purs", source)]);
    let (_, diagnostics) = check_env(&modules);
    assert!(
        codes(&diagnostics).contains(&"KindsDoNotUnify"),
        "{diagnostics:?}"
    );
}

#[test]
fn a_synonym_forall_whose_kind_is_generalized_is_accepted() {
    // `a`'s kind is the parameter kind `Natural` quantifies
    // (`forall k. (k -> Type) -> (k -> Type) -> Type`), not an unknown left
    // in the body.
    check_ok(
        "\
module Main where
type Natural f g = forall a. f a -> g a
",
    );
}

#[test]
fn a_monomorphic_binding_group_with_no_bad_use_is_accepted() {
    check_ok(
        "\
module Main where
data F (a :: Type -> Type) = F
data A a = A (B a)
type B a = F A
",
    );
}

#[test]
fn a_synonym_kind_cannot_quantify_a_visible_type_variable() {
    // `failing/QuantificationCheckFailure3.purs`. `(TProxy :: KProxy k)` makes
    // the synonym kind mention `k`. That is visible dependent quantification,
    // not an unsolved kind unknown.
    let source =
        include_str!("../../../../tests/upstream/failing/QuantificationCheckFailure3.purs");
    let modules = resolve_program(&[("Main.purs", source)]);
    let (_, diagnostics) = check_env(&modules);
    let found = codes(&diagnostics);
    assert!(
        found.contains(&"VisibleQuantificationCheckFailureInType"),
        "{diagnostics:?}"
    );
    assert!(
        !found.contains(&"QuantificationCheckFailureInType"),
        "the kind annotation solves the polykind: {diagnostics:?}"
    );
}

#[test]
fn a_kind_annotation_checks_its_type_and_keeps_a_bound_quantifier() {
    // `Proxy`'s parameter kind is generalized. The annotation forces it to
    // `Type`, and a quantifier written inside the annotation is not a free
    // type variable of the synonym.
    check_ok(
        "\
module Main where
data Proxy a = Proxy
type Id = (Proxy :: Type -> Type)
type Wrapped = (Int :: Type)
",
    );
}

#[test]
fn a_kind_annotation_rejects_a_different_kind() {
    let source = "\
module Main where
type T = (Int :: Type -> Type)
";
    let modules = resolve_program(&[("Main.purs", source)]);
    let (_, diagnostics) = check_env(&modules);
    let found = codes(&diagnostics);
    assert!(found.contains(&"KindsDoNotUnify"), "{diagnostics:?}");
    assert!(
        !found.contains(&"VisibleQuantificationCheckFailureInType"),
        "{diagnostics:?}"
    );
}

#[test]
fn an_unsolved_kind_inside_a_synonym_forall_is_rejected() {
    let source = "\
module Main where
data Proxy a = Proxy
type T = forall a. Proxy a
";
    let modules = resolve_program(&[("Main.purs", source)]);
    let (_, diagnostics) = check_env(&modules);
    assert!(
        codes(&diagnostics).contains(&"QuantificationCheckFailureInType"),
        "{diagnostics:?}"
    );
}
