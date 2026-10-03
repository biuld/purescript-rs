//! Focused rule tests for the deferred row relations.

use super::*;
use crate::typecheck::classes::UnsolvedPolicy;

#[test]
fn lacks_proves_absence_in_an_empty_row_even_for_an_unknown_label() {
    let mut checker = checker();
    let unknown_label = unknown();
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_LACKS,
        vec![unknown_label.clone(), InferType::RowEmpty],
    );

    let PrimitiveDispatch::Solved(WantedSolution::Primitive { arguments }) =
        checker.solve_primitive(&goal, SolveDepth::new())
    else {
        panic!("an empty row lacks every label");
    };
    assert_eq!(arguments, vec![unknown_label, InferType::RowEmpty]);
    assert!(checker.state.errors.is_empty());
}

#[test]
fn lacks_rejects_a_known_label_even_when_the_tail_is_open() {
    let mut checker = checker();
    let row = single_with("a", int(), unknown());
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_LACKS,
        vec![label("a"), row],
    );

    assert!(matches!(
        checker.solve_primitive(&goal, SolveDepth::new()),
        PrimitiveDispatch::Reported
    ));
    assert_eq!(checker.state.errors.len(), 1);
    assert_eq!(
        checker.state.errors[0].error_code(),
        Some("NoInstanceFound")
    );
    assert_eq!(checker.state.errors[0].span, goal.span);
}

#[test]
fn lacks_defers_known_absence_to_an_open_tail() {
    let mut checker = checker();
    let tail = unknown();
    let row = single_with("b", int(), tail.clone());
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_LACKS,
        vec![label("a"), row.clone()],
    );

    assert!(matches!(
        checker.solve_primitive(&goal, SolveDepth::new()),
        PrimitiveDispatch::Deferred {
            evidence: Some(WantedSolution::Primitive { .. })
        }
    ));
    assert_eq!(checker.state.wanted.len(), 1);
    let residual = checker.state.wanted[0].clone();
    assert_eq!(residual.class_id, hir::TypeId::PRIM_ROW_LACKS);
    assert_eq!(residual.arguments, vec![label("a"), tail]);
    assert_eq!(residual.span, goal.span);
    assert!(residual.solution.is_none());
    assert!(checker.state.errors.is_empty());
    assert_eq!(checker.resolve_type(goal.arguments[1].clone()), row);
}

#[test]
fn an_inferred_lacks_residual_is_generalized_by_the_root_worklist() {
    let mut checker = checker();
    let row = single_with("b", int(), unknown());
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_LACKS,
        vec![label("a"), row],
    );
    checker.state.wanted.push(goal);

    let retained = checker.solve_wanted_constraints(None, 0, UnsolvedPolicy::Retain);

    assert_eq!(retained, vec![1]);
    assert_eq!(checker.state.wanted.len(), 2);
    assert!(checker.state.wanted[0].solution.is_some());
    assert!(checker.state.wanted[1].solution.is_none());
    assert_eq!(
        checker.state.wanted[1].class_id,
        hir::TypeId::PRIM_ROW_LACKS
    );
    assert!(checker.state.errors.is_empty());
}

#[test]
fn union_merges_a_closed_left_row_without_reordering_fields() {
    let mut checker = checker();
    let left = row(("b", boolean()), ("a", int()));
    let right = single("a", boolean());
    let output = unknown();
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_UNION,
        vec![left.clone(), right.clone(), output.clone()],
    );
    let expected = single_with("b", boolean(), single_with("a", int(), right));

    let PrimitiveDispatch::Solved(WantedSolution::Primitive { arguments }) =
        checker.solve_primitive(&goal, SolveDepth::new())
    else {
        panic!("a closed left row is merged into the right row");
    };
    assert_eq!(
        arguments,
        vec![left, goal.arguments[1].clone(), expected.clone()]
    );
    assert_eq!(checker.resolve_type(output), expected);
    assert!(checker.state.errors.is_empty());
}

#[test]
fn union_splits_closed_right_and_output_rows_by_label() {
    let mut checker = checker();
    let inferred_left = unknown();
    let output = row(("b", boolean()), ("a", int()));
    let right = single("a", int());
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_UNION,
        vec![inferred_left.clone(), right.clone(), output.clone()],
    );
    let expected_left = single("b", boolean());
    let expected_right = single("a", int());

    let PrimitiveDispatch::Solved(WantedSolution::Primitive { arguments }) =
        checker.solve_primitive(&goal, SolveDepth::new())
    else {
        panic!("closed right and output rows determine both inputs");
    };
    assert_eq!(arguments[0], expected_left);
    assert_eq!(arguments[1], expected_right);
    assert_eq!(arguments[2], output);
    assert_eq!(checker.resolve_type(inferred_left), expected_left);
    assert!(checker.state.errors.is_empty());
}

#[test]
fn union_defers_an_open_left_remainder_with_the_same_element_kind() {
    let mut checker = checker();
    let tail = unknown();
    let left = single_with("a", label("field"), tail.clone());
    let right = checker.fresh();
    let output = checker.fresh();
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_UNION,
        vec![left.clone(), right.clone(), output.clone()],
    );

    assert!(matches!(
        checker.solve_primitive(&goal, SolveDepth::new()),
        PrimitiveDispatch::Deferred {
            evidence: Some(WantedSolution::Primitive { .. })
        }
    ));
    assert_eq!(checker.state.wanted.len(), 1);
    let residual = checker.state.wanted[0].clone();
    assert_eq!(residual.class_id, hir::TypeId::PRIM_ROW_UNION);
    assert_eq!(residual.arguments[0], tail);
    assert_eq!(residual.arguments[1], right);
    let fresh_tail = residual.arguments[2].clone();
    let expected_prefix = single_with("a", label("field"), fresh_tail.clone());
    assert_eq!(checker.resolve_type(output), expected_prefix);
    let original_kind = checker.kind_of_type(&left, goal.span);
    let residual_kind = checker.kind_of_type(&fresh_tail, goal.span);
    assert_eq!(
        original_kind.map(|kind| checker.state.kinds.resolve(kind)),
        residual_kind.map(|kind| checker.state.kinds.resolve(kind))
    );
    assert_eq!(residual.span, goal.span);
    assert!(residual.solution.is_none());
    assert!(checker.state.errors.is_empty());
}

#[test]
fn lacks_and_union_decline_open_rows_without_known_fields() {
    for class_id in [hir::TypeId::PRIM_ROW_LACKS, hir::TypeId::PRIM_ROW_UNION] {
        let mut checker = checker();
        let args = match class_id {
            hir::TypeId::PRIM_ROW_LACKS => vec![label("a"), unknown()],
            _ => vec![unknown(), unknown(), unknown()],
        };
        let goal = goal(&mut checker, class_id, args);
        let substitutions = checker.state.substitutions.clone();
        let levels = checker.state.levels.clone();
        let kinds = checker.state.variable_kinds.clone();

        assert!(matches!(
            checker.solve_primitive(&goal, SolveDepth::new()),
            PrimitiveDispatch::None
        ));
        assert_eq!(checker.state.substitutions, substitutions);
        assert_eq!(checker.state.levels, levels);
        assert_eq!(checker.state.variable_kinds, kinds);
        assert!(checker.state.wanted.is_empty());
        assert!(checker.state.errors.is_empty());
    }
}
