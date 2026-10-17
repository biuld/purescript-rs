//! The `Prim.Coerce.Coercible` rule's contract, exercised from the table: it is
//! a real rule written against the framework the synthetic cases in
//! `outcomes` cover, so its dispatch questions are asked here.

use super::*;

/// Dispatches one obligation through the rule table.
fn dispatch(checker: &mut Checker, constraint: &WantedConstraint) -> PrimitiveDispatch {
    checker.solve_primitive(constraint, SolveDepth::new())
}

#[test]
fn a_coercible_obligation_reaches_the_rule_from_the_table() {
    let mut checker = checker();
    let source = checker.fresh();
    let target = checker.fresh();
    let mut constraint = obligation(&mut checker, hir::TypeId::COERCIBLE, vec![source, target]);
    assert_eq!(
        primitive_rule(hir::TypeId::COERCIBLE).map(|r| r.arity),
        Some(2)
    );

    // Two unknown arguments the role analysis cannot relate are undecided, so the
    // obligation continues into the ordinary paths rather than being reported.
    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::None
    ));
    assert!(checker.state.errors.is_empty());

    // Equal known arguments are proved, and the proof is the evidence.
    constraint.arguments = vec![int(), int()];
    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::Solved(WantedSolution::Coercible { .. })
    ));
    assert!(checker.state.errors.is_empty());
}

#[test]
fn a_coercible_obligation_whose_arguments_have_different_kinds_reports_kinds_do_not_unify() {
    // Official solving unifies a picked-up `Coercible` constraint's argument
    // kinds (`Entailment/Coercible.hs`), so `Coercible Int "label"` is a kind
    // failure at that exact check rather than a missing instance: a `Type` and
    // a `Symbol` share no kind, and no instance could ever decide the pair.
    let mut checker = checker();
    let constraint = obligation(
        &mut checker,
        hir::TypeId::COERCIBLE,
        vec![
            int(),
            // A type-level string has kind `Symbol`, which no `Type`-kinded
            // argument can share.
            InferType::TypeLevelString("label".to_owned()),
        ],
    );
    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::Reported
    ));
    assert_eq!(checker.state.errors.len(), 1);
    let error = &checker.state.errors[0];
    assert_eq!(
        error.kind,
        TypeCheckErrorKind::KindsDoNotUnify,
        "the verdict is the kind equation's, not a missing instance"
    );
    assert_eq!(error.error_code(), Some("KindsDoNotUnify"));
    assert_eq!(
        error.span, constraint.span,
        "the verdict names the obligation's own range"
    );
}
