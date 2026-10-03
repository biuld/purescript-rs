//! The four outcomes, and the refusals that keep an unknown argument from being
//! decisive.

use super::*;

fn dispatch(checker: &mut Checker, constraint: &WantedConstraint) -> PrimitiveDispatch {
    checker.solve_primitive(constraint, SolveDepth::new())
}

#[test]
fn a_solved_outcome_keeps_the_evidence_the_rule_produced() {
    let mut checker = checker();
    let constraint = obligation(&mut checker, SOLVING, vec![int(), int()]);

    let PrimitiveDispatch::Solved(WantedSolution::Primitive { arguments }) =
        dispatch(&mut checker, &constraint)
    else {
        panic!("a rule that decides the relation must discharge it");
    };
    assert_eq!(arguments, vec![int(), int()]);
    assert!(checker.state.errors.is_empty());
}

#[test]
fn an_undecided_outcome_falls_through_and_leaves_no_state_behind() {
    let mut checker = checker();
    let first = checker.fresh();
    let second = checker.fresh();
    let constraint = obligation(&mut checker, DECLINING, vec![first, second]);
    let before = SolverFingerprint::of(&checker);

    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::None
    ));
    assert_eq!(
        SolverFingerprint::of(&checker),
        before,
        "a declined rule leaves no substitution, level, kind, or diagnostic behind"
    );
    assert!(
        checker.state.wanted.is_empty(),
        "a declined rule emits no obligation"
    );
}

#[test]
fn a_deferred_outcome_keeps_its_evidence_and_requeues_the_obligation() {
    let mut checker = checker();
    let constraint = obligation(&mut checker, DEFERRING, vec![int(), int()]);

    let PrimitiveDispatch::Deferred { evidence } = dispatch(&mut checker, &constraint) else {
        panic!("a rule that defers with evidence must report a deferral");
    };
    assert!(matches!(evidence, Some(WantedSolution::Primitive { .. })));
    assert_eq!(
        checker.state.wanted.len(),
        1,
        "the deferred obligation is re-queued and retained"
    );
    let requeued = &checker.state.wanted[0];
    assert_eq!(requeued.class_id, SOLVING);
    assert_eq!(
        requeued.span, constraint.span,
        "the re-queued obligation keeps its own origin"
    );
    assert!(
        matches!(requeued.solution, Some(WantedSolution::Primitive { .. })),
        "the re-queued obligation is decided by its own re-entry"
    );
    assert!(checker.state.errors.is_empty());
}

#[test]
fn a_failed_outcome_is_reported_under_the_rule_s_own_code_and_the_obligation_range() {
    let mut checker = checker();
    let constraint = obligation(&mut checker, FAILING, vec![int(), int()]);

    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::Reported
    ));
    assert_eq!(checker.state.errors.len(), 1);
    let error = &checker.state.errors[0];
    assert_eq!(error.kind, TypeCheckErrorKind::NoInstance);
    assert_eq!(error.span, constraint.span);
    assert_eq!(error.error_code(), Some("NoInstanceFound"));
    assert!(error.message().contains("does not hold"));
}

#[test]
fn a_failure_with_an_unknown_argument_is_not_reported_as_impossible() {
    let mut checker = checker();
    let unknown = checker.fresh();
    let constraint = obligation(&mut checker, FAILING, vec![int(), unknown]);

    assert!(
        matches!(dispatch(&mut checker, &constraint), PrimitiveDispatch::None),
        "an obligation with an unknown argument is undecided, not impossible"
    );
    assert!(
        checker.state.errors.is_empty(),
        "the obligation must reach instance search instead of a diagnostic"
    );
}

#[test]
fn a_failure_under_a_code_with_no_official_error_code_is_refused() {
    let mut checker = checker();
    let constraint = obligation(
        &mut checker,
        FAILING_WITH_AN_INVENTED_CODE,
        vec![int(), int()],
    );

    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::None
    ));
    assert!(checker.state.errors.is_empty());
}

#[test]
fn a_solution_that_determined_nothing_is_read_as_the_deferral_it_should_be() {
    let mut checker = checker();
    let unknown = checker.fresh();
    let constraint = obligation(&mut checker, SOLVING_WITHOUT_PROGRESS, vec![int(), unknown]);

    let PrimitiveDispatch::Deferred { evidence } = dispatch(&mut checker, &constraint) else {
        panic!("a rule that decided nothing has deferred");
    };
    assert!(
        evidence.is_none(),
        "there is no decision to record as evidence"
    );
    assert_eq!(
        checker.state.wanted.len(),
        1,
        "its obligations are re-queued"
    );
    assert!(checker.state.errors.is_empty());
}

#[test]
fn deciding_the_known_part_and_deferring_the_rest_is_a_solution() {
    let mut checker = checker();
    let unknown = checker.fresh();
    let constraint = obligation(
        &mut checker,
        DECIDING_AND_DEFERRING,
        vec![int(), int(), unknown],
    );

    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::Solved(WantedSolution::Primitive { .. })
    ));
    assert_eq!(
        checker.state.wanted.len(),
        1,
        "the part the rule could not decide is re-queued"
    );
    assert!(checker.state.errors.is_empty());
}

#[test]
fn a_deferral_that_returns_the_obligation_it_was_asked_about_is_reported() {
    let mut checker = checker();
    let unknown = checker.fresh();
    let constraint = obligation(&mut checker, DEFERRING_TO_ITSELF, vec![int(), unknown]);

    let PrimitiveDispatch::Deferred { evidence } = dispatch(&mut checker, &constraint) else {
        panic!("the outer deferral is still a deferral");
    };
    assert!(evidence.is_none());
    assert_eq!(checker.state.errors.len(), 1);
    let error = &checker.state.errors[0];
    assert_eq!(error.kind, TypeCheckErrorKind::NoInstance);
    assert_eq!(error.error_code(), Some("NoInstanceFound"));
    assert_eq!(error.span, constraint.span);
    assert!(
        error
            .message()
            .contains("deferred without determining an argument"),
        "{}",
        error.message()
    );
}

#[test]
fn a_wanted_constraint_that_is_not_the_member_s_own_is_not_this_rule_s_obligation() {
    let mut checker = checker();
    let constraint = obligation(&mut checker, SOLVING, vec![int()]);

    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::None
    ));
    assert!(checker.state.errors.is_empty());
}

#[test]
fn a_proof_member_is_dispatched_before_the_givens_and_a_relation_after_them() {
    assert!(
        primitive_rule_precedes_givens(hir::TypeId::COERCIBLE),
        "a Proof member's evidence is a boundary, not a dictionary"
    );
    for rule in SYNTHETIC {
        assert!(
            !primitive_rule_precedes_givens(rule.class_id),
            "a relation's dictionary is what a given supplies"
        );
    }
    assert!(
        !primitive_rule_precedes_givens(hir::TypeId::PRIM_ROW_LACKS),
        "a member with no rule is not dispatched at all"
    );
}

#[test]
fn every_outcome_is_reachable_and_only_official_codes_are_reportable() {
    // The four outcomes are the contract, so the cases above are what keeps each
    // one of them a live answer rather than a variant no rule can return.
    assert_eq!(SYNTHETIC.len(), 8);
    assert!(EvidenceClass::RuntimeDictionary.accepts_a_given());
    assert!(EvidenceClass::ReportOnly.accepts_a_given());
    assert!(!EvidenceClass::CompileTimeProof.accepts_a_given());
    assert_eq!(
        TypeCheckErrorKind::NoInstance.error_code(),
        Some("NoInstanceFound")
    );
    assert_eq!(TypeCheckErrorKind::FundepConflict.error_code(), None);
    assert!(
        PrimitiveEvidence::Report.into_solution().is_none(),
        "a report's result is its diagnostic, not evidence"
    );
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
