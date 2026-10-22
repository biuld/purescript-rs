//! Implicit kind variables are quantified before explicit binders. A sort that
//! mentions a binder not yet in scope is `QuantificationCheckFailureInKind`.

use super::{check_env, codes, resolve_program};
use crate::kind::Kind;
use std::collections::HashSet;

const QUANTIFICATION: &str =
    include_str!("../../../../tests/upstream/failing/QuantificationCheckFailure.purs");

#[test]
fn an_implicit_kind_before_its_binder_is_rejected() {
    let modules = resolve_program(&[("Main.purs", QUANTIFICATION)]);
    let (environment, diagnostics) = check_env(&modules);
    assert!(
        codes(&diagnostics).contains(&"QuantificationCheckFailureInKind"),
        "{diagnostics:?}"
    );
    let declaration = modules[0]
        .types
        .iter()
        .find(|declaration| declaration.name == "T")
        .expect("T");
    let scheme = environment
        .kind_scheme(declaration.id)
        .expect("T's kind scheme");
    let explicit: HashSet<u32> = scheme.variables.iter().copied().collect();
    let implicit = scheme
        .binders
        .first()
        .expect("the implicit unknown is the first binder");
    assert!(
        !explicit.contains(&implicit.variable),
        "the first binder is an explicit parameter: {scheme:?}"
    );
    let mentions_explicit = free_variables(&implicit.kind)
        .into_iter()
        .any(|variable| explicit.contains(&variable));
    assert!(
        mentions_explicit,
        "the implicit sort should name an explicit binder: {scheme:?}"
    );
}

#[test]
fn a_polykinded_proxy_signature_stays_well_scoped() {
    let modules = resolve_program(&[(
        "Main.purs",
        "module Main where\n\
         data Proxy :: forall k. k -> Type\n\
         data Proxy a = Proxy\n\
         data Relate :: forall a (b :: a). a -> Proxy b -> Type\n\
         data Relate x y = Relate\n",
    )]);
    let (environment, diagnostics) = check_env(&modules);
    assert!(
        diagnostics.is_empty(),
        "a well-scoped signature was rejected: {diagnostics:?}"
    );
    let relate = modules[0]
        .types
        .iter()
        .find(|declaration| declaration.name == "Relate")
        .expect("Relate");
    let scheme = environment.kind_scheme(relate.id).expect("Relate's scheme");
    assert_eq!(scheme.binders.len(), scheme.variables.len());
    assert!(
        scheme
            .binders
            .iter()
            .all(|binder| scheme.variables.contains(&binder.variable)),
        "Relate should quantify only its explicit binders: {scheme:?}"
    );
}

fn free_variables(kind: &Kind) -> Vec<u32> {
    let mut out = Vec::new();
    walk(kind, &mut out);
    out
}

fn walk(kind: &Kind, out: &mut Vec<u32>) {
    match kind {
        Kind::Variable(variable) => out.push(*variable),
        Kind::App(function, argument) | Kind::Function(function, argument) => {
            walk(function, out);
            walk(argument, out);
        }
        Kind::ForAll { body, .. } => walk(body, out),
        Kind::RowExtend { field, tail, .. } => {
            walk(field, out);
            walk(tail, out);
        }
        Kind::Builtin(_) | Kind::Named(_) | Kind::RowEmpty | Kind::TypeVariable(_) => {}
    }
}
