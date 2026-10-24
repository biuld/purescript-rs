use super::*;
use crate::typecheck::{ClassConstraint, TypeCheckError, TypeCheckErrorKind, TypeCheckWarning};
use psrs_span::TextRange;

#[test]
fn leaving_a_given_scope_retains_skolems_created_by_its_body() {
    let mut checker = super::checker();
    let argument = checker.fresh();
    let InferType::Variable(given_variable) = argument else {
        unreachable!()
    };
    let constraint = ClassConstraint {
        class_id: hir::TypeId::COERCIBLE,
        arguments: vec![argument.clone(), argument],
        span: TextRange::new(0, 1),
    };
    let skolem = checker.with_givens(
        vec![(constraint, WantedSolution::Given(LocalId(42)))],
        |checker| {
            assert!(checker.state.rigid.contains(&given_variable));
            let InferType::Variable(skolem) = checker.fresh() else {
                unreachable!()
            };
            checker.with_skolem_scope(&[skolem], |_| ());
            skolem
        },
    );
    assert!(!checker.state.rigid.contains(&given_variable));
    assert!(checker.state.rigid.contains(&skolem));
    assert!(checker.scope.givens.is_empty());
    assert!(checker.scope.given_rigid.is_empty());
}

#[test]
fn lexical_scope_restores_local_bindings_without_removing_its_parent() {
    let mut checker = super::checker();
    let parent = Scheme::monomorphic(InferType::Constructor(TypeConstructor::Int));
    checker.scope.locals.insert(LocalId(0), parent.clone());
    checker.with_scope(|checker| {
        checker.scope.locals.insert(LocalId(1), parent.clone());
        checker.scope.locals.remove(&LocalId(0));
    });
    assert_eq!(checker.scope.locals.len(), 1);
    assert_eq!(checker.scope.locals[&LocalId(0)].ty, parent.ty);
}

#[test]
fn failed_candidate_restores_solver_and_lexical_state() {
    let mut checker = super::checker();
    let next_variable = checker.state.next_variable;
    let next_wanted_id = checker.state.next_wanted_id;
    let next_local = checker.state.next_dictionary_local;
    let annotation_variables = checker.scope.annotation_variables.clone();
    let kinds = checker.state.kinds.clone();
    let variable_kinds = checker.state.variable_kinds.clone();
    let levels = checker.state.levels.clone();
    let span = TextRange::new(4, 8);

    assert_eq!(
        checker.speculate(|checker| {
            let argument = checker.fresh();
            checker.state.next_dictionary_local += 1;
            checker
                .scope
                .annotation_variables
                .insert("trial".into(), argument.clone());
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
                code: "UserDefinedWarning",
            });
            None::<()>
        }),
        None
    );

    assert_eq!(checker.state.next_variable, next_variable);
    assert_eq!(checker.state.next_wanted_id, next_wanted_id);
    assert_eq!(checker.state.next_dictionary_local, next_local);
    assert_eq!(checker.scope.annotation_variables, annotation_variables);
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
fn failed_annotation_candidate_restores_state_and_keeps_diagnostic() {
    let mut checker = super::checker();
    let next_variable = checker.state.next_variable;
    let span = TextRange::new(9, 13);

    assert_eq!(
        checker.speculate_reporting(|checker| {
            checker.fresh();
            checker.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TypeMismatch,
                span,
                "annotation does not typecheck",
            ));
            None::<()>
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
