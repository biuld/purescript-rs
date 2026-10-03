use super::*;
use crate::typecheck::{ClassConstraint, TypeCheckError, TypeCheckErrorKind, TypeCheckWarning};
use psrs_span::TextRange;

#[test]
fn successful_annotation_probe_rolls_back_solver_scope_and_reports() {
    let mut checker = super::checker();
    let next_variable = checker.state.next_variable;
    let next_wanted_id = checker.state.next_wanted_id;
    let kinds = checker.state.kinds.clone();
    let variable_kinds = checker.state.variable_kinds.clone();
    let levels = checker.state.levels.clone();
    let span = TextRange::new(4, 8);

    assert_eq!(
        checker.probe_reporting(|checker| {
            let argument = checker.fresh();
            let constraint = ClassConstraint {
                class_id: hir::TypeId::PRIM_PARTIAL,
                arguments: Vec::new(),
                span,
            };
            let wanted = checker.build_constraint(hir::TypeId::PRIM_PARTIAL, vec![argument], span);
            checker.state.wanted.push(wanted);
            checker
                .scope
                .givens
                .push((constraint, WantedSolution::Given(LocalId(42))));
            checker.state.warnings.push(TypeCheckWarning {
                span,
                message: "probe warning".into(),
            });
            Some(())
        }),
        Some(())
    );

    assert_eq!(checker.state.next_variable, next_variable);
    assert_eq!(checker.state.next_wanted_id, next_wanted_id);
    assert_eq!(format!("{:?}", checker.state.kinds), format!("{kinds:?}"));
    assert_eq!(checker.state.variable_kinds, variable_kinds);
    assert_eq!(checker.state.levels, levels);
    assert!(checker.state.wanted.is_empty());
    assert!(checker.scope.givens.is_empty());
    assert!(checker.state.warnings.is_empty());
    assert!(checker.state.errors.is_empty());
    assert_eq!(checker.fresh(), InferType::Variable(next_variable));
}

#[test]
fn rejected_annotation_probe_restores_state_and_keeps_diagnostic() {
    let mut checker = super::checker();
    let next_variable = checker.state.next_variable;
    let span = TextRange::new(9, 13);

    assert_eq!(
        checker.probe_reporting(|checker| {
            checker.fresh();
            checker.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TypeMismatch,
                span,
                "annotation does not typecheck",
            ));
            None
        }),
        None
    );

    assert_eq!(checker.state.next_variable, next_variable);
    assert_eq!(checker.fresh(), InferType::Variable(next_variable));
    assert_eq!(checker.state.errors.len(), 1);
    assert_eq!(
        checker.state.errors[0].kind,
        TypeCheckErrorKind::TypeMismatch
    );
    assert_eq!(checker.state.errors[0].span, span);
}
