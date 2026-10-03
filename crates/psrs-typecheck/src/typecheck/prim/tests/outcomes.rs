//! The four outcomes, and the refusals that keep an unknown argument from being
//! decisive.

use super::*;
use crate::typecheck::TypeInterner;

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
fn a_rule_owned_failure_is_reported_even_when_an_unrelated_argument_is_unknown() {
    let mut checker = checker();
    let unknown = checker.fresh();
    let constraint = obligation(&mut checker, FAILING, vec![int(), unknown]);

    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::Reported
    ));
    assert_eq!(checker.state.errors.len(), 1);
    assert_eq!(
        checker.state.errors[0].error_code(),
        Some("NoInstanceFound")
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

/// A rule that decides nothing is downgraded, and leaves nothing behind.
///
/// This is the other half of the contract the framework's check has to preserve:
/// a relation's decided arguments are unified against the goal's first, and when
/// that unification proves nothing — because the decision only repeats what the
/// goal already said — "nothing became more determined" still means the rule
/// decided nothing, so the answer is read as the deferral it should have been and
/// nothing survives it.
#[test]
fn an_explicit_solution_is_not_reinterpreted_as_a_deferral_from_progress() {
    let mut checker = checker();
    let unknown = checker.fresh();
    let constraint = obligation(
        &mut checker,
        SOLVING_WITHOUT_PROGRESS,
        vec![int(), unknown.clone()],
    );
    let PrimitiveDispatch::Solved(WantedSolution::Primitive { arguments }) =
        dispatch(&mut checker, &constraint)
    else {
        panic!("the rule explicitly decided the relation");
    };
    assert_eq!(arguments, vec![int(), unknown]);
    assert_eq!(
        checker.state.wanted.len(),
        1,
        "the rule's explicit residual obligation is re-queued"
    );
    assert!(checker.state.wanted[0].solution.is_some());
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
fn equal_sibling_residuals_are_each_solved_once() {
    let mut checker = checker();
    let constraint = obligation(&mut checker, DEFERRING_TO_SIBLINGS, vec![int(), int()]);

    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::Deferred { evidence: None }
    ));
    assert_eq!(checker.state.wanted.len(), 2);
    assert!(
        checker
            .state
            .wanted
            .iter()
            .all(|wanted| wanted.solution.is_some())
    );
    assert!(checker.state.errors.is_empty());
}

#[test]
fn the_requeued_width_budget_is_shared_across_a_branching_tree() {
    let mut checker = checker();
    let constraint = obligation(&mut checker, GROWING_DEFERRAL, vec![int(), int()]);

    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::Deferred { evidence: None }
    ));
    assert_eq!(
        checker.state.next_variable, 66,
        "the root and exactly 32 re-entered rule calls each allocate two fresh residual variables"
    );
    assert!(!checker.state.errors.is_empty());
    assert!(checker.state.wanted.is_empty());
}

#[test]
fn a_root_worklist_does_not_restart_a_reported_self_deferral() {
    let mut checker = checker();
    let unknown = checker.fresh();
    let constraint = obligation(&mut checker, DEFERRING_TO_ITSELF, vec![int(), unknown]);
    checker.state.wanted.push(constraint);

    let retained = checker.solve_wanted_constraints(
        None,
        0,
        crate::typecheck::classes::UnsolvedPolicy::Retain,
    );

    assert!(retained.is_empty());
    assert_eq!(checker.state.wanted.len(), 1);
    assert_eq!(checker.state.errors.len(), 1);
    assert_eq!(
        checker.state.errors[0].error_code(),
        Some("NoInstanceFound")
    );
}

#[test]
fn a_missing_nested_wanted_reference_is_an_explicit_evidence_error() {
    let mut checker = checker();
    let mut constraint = obligation(&mut checker, SOLVING, vec![int(), int()]);
    constraint.solution = Some(WantedSolution::Instance {
        constructor: psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
        constructor_type: InferType::RowEmpty,
        context: vec![u32::MAX],
    });
    checker.state.wanted.push(constraint);
    let mut interner = TypeInterner::default();

    assert!(
        checker
            .wanted_evidence(0, &mut interner, &HashSet::new())
            .is_none()
    );
    assert_eq!(checker.state.errors.len(), 1);
    assert_eq!(checker.state.errors[0].kind, TypeCheckErrorKind::InvalidHir);
    assert!(
        checker.state.errors[0]
            .message()
            .contains("missing wanted constraint")
    );
}

/// The framework binds what a rule decided, rather than believing that it did.
///
/// This is official `Entailment.hs:301`: the dictionary carries the type the rule
/// decided, the framework unifies it against the goal's argument, and that
/// unification is the binding. A rule that assigned its decision to the wanted
/// argument would make that step untestable, because there would be nothing left
/// for it to check.
#[test]
fn a_stated_decision_is_bound_by_the_framework() {
    let mut checker = checker();
    let open = checker.fresh();
    let constraint = obligation(&mut checker, DECIDING, vec![open.clone(), int()]);

    let PrimitiveDispatch::Solved(WantedSolution::Primitive { arguments }) =
        dispatch(&mut checker, &constraint)
    else {
        panic!("a stated decision for an open argument discharges the obligation");
    };
    assert_eq!(
        checker.resolve_type(open),
        boolean(),
        "the binding came from unifying the decided argument against the goal's"
    );
    assert_eq!(
        arguments,
        vec![boolean(), int()],
        "the evidence records the arguments the constraint now has"
    );
    assert!(checker.state.errors.is_empty());
}

/// A decision that contradicts an argument the obligation already fixed is
/// reported, and the solver is *not* restored.
///
/// This is the case the two refusals cannot see: the rule bound nothing, so
/// "nothing became more determined" reads as "the rule decided nothing", and the
/// contradiction is dropped with the snapshot — which is how a decided value that
/// is wrong became `no instance for constraint`. The framework's own unification
/// finds it instead, keeps its diagnostic, and refuses the obligation. It is the
/// mirror of the decline case above, which must still leave nothing behind.
#[test]
fn a_decision_that_contradicts_a_known_argument_is_reported_and_kept() {
    let mut checker = checker();
    let constraint = obligation(&mut checker, CONTRADICTING, vec![int(), int()]);
    let next_variable = checker.state.next_variable;

    assert!(
        matches!(
            dispatch(&mut checker, &constraint),
            PrimitiveDispatch::Reported
        ),
        "a contradiction is a report, not a decline: the obligation cannot hold"
    );
    assert_eq!(
        checker.state.next_variable,
        next_variable + 1,
        "the snapshot was not restored, so the rule's allocation stands behind \
         the diagnostic rather than being discarded with it"
    );
    assert_eq!(checker.state.errors.len(), 1);
    let error = &checker.state.errors[0];
    assert_eq!(
        error.kind,
        TypeCheckErrorKind::TypeMismatch,
        "the diagnostic is the shared unifier's, not a rule's restatement"
    );
    assert_eq!(error.error_code(), Some("TypesDoNotUnify"));
    assert_eq!(
        error.span, constraint.span,
        "the diagnostic keeps the obligation's own range"
    );
}

/// The unifier's binding is rolled back when the check finds a contradiction, so a
/// rule's speculative work does not survive describing an obligation that cannot
/// hold. What survives is the diagnostic, deliberately.
#[test]
fn a_contradiction_keeps_the_diagnostic_and_drops_the_partial_binding() {
    let mut checker = checker();
    let open = checker.fresh();
    // The first argument is open, so the check binds it before it reaches the
    // second, which the goal already fixed to something the decision contradicts.
    let constraint = obligation(&mut checker, CONTRADICTING, vec![open.clone(), int()]);
    let substitutions = checker.state.substitutions.clone();

    assert!(matches!(
        dispatch(&mut checker, &constraint),
        PrimitiveDispatch::Reported
    ));
    assert_eq!(
        checker.resolve_type(open.clone()),
        open,
        "the check that failed leaves no binding behind"
    );
    assert_eq!(checker.state.substitutions, substitutions);
    assert_eq!(checker.state.errors.len(), 1);
    assert_eq!(
        checker.state.errors[0].kind,
        TypeCheckErrorKind::TypeMismatch
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
fn proof_and_relation_rules_precede_dictionary_givens() {
    assert!(
        primitive_rule_precedes_givens(hir::TypeId::COERCIBLE),
        "a Proof member's evidence is a boundary, not a dictionary"
    );
    for rule in SYNTHETIC {
        assert!(
            primitive_rule_precedes_givens(rule.class_id),
            "relations decide their type-level facts before a given dictionary can mask them"
        );
    }
    assert!(
        primitive_rule_precedes_givens(hir::TypeId::PRIM_ROW_LACKS),
        "the row relation rule is registered and precedes a given"
    );
}

#[test]
fn every_outcome_is_reachable_and_only_official_codes_are_reportable() {
    // The four outcomes are the contract, so the cases above are what keeps each
    // one of them a live answer rather than a variant no rule can return.
    assert_eq!(SYNTHETIC.len(), 12);
    assert!(EvidenceClass::RuntimeDictionary.accepts_a_given());
    assert!(EvidenceClass::ReportingDictionary.accepts_a_given());
    assert!(!EvidenceClass::ReportingDictionary.precedes_givens());
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
