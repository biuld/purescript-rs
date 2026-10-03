//! The three row relations at the level of the obligations a rule receives: the
//! reading each one decides, the declines it makes, and the contradictions the
//! framework reports for it.

use super::*;
use crate::typecheck::classes::SolveDepth;
use crate::typecheck::prim::PrimitiveDispatch;
use psrs_span::SourceFile;

/// A checker over an empty module, so a rule can be consulted directly on a
/// goal built by hand rather than through inference.
fn checker() -> Checker {
    let file = SourceFile::new("Main.purs", "module Main where\n");
    let (tokens, errors) = psrs_syntax::lex(file.text());
    assert!(errors.is_empty(), "lex errors: {errors:?}");
    let layout = psrs_syntax::add_layout(&file, &tokens);
    let cst = psrs_syntax::parse_module(&layout).expect("parse");
    let ast = psrs_ast::lower_module(cst).expect("surface lowering");
    let resolved = psrs_resolve::resolve_module_with_externals(
        ast,
        hir::ModuleId(0),
        &psrs_resolve::bootstrap_externals(),
    )
    .expect("the program should resolve");
    Checker::new(
        &resolved,
        &HashMap::new(),
        TypecheckContext {
            known_types: &[],
            imported_instances: &[],
            module_names: &HashMap::from([(hir::ModuleId(0), "Main".to_owned())]),
            checked_kinds: &psrs_kind::CheckedKindEnv::default(),
        },
    )
}

/// A wanted obligation over `arguments`, with the range every diagnostic about it
/// and every piece of evidence it produces carries.
fn goal(
    checker: &mut Checker,
    class_id: hir::TypeId,
    arguments: Vec<InferType>,
) -> WantedConstraint {
    let constraint = ClassConstraint {
        class_id,
        arguments,
        span: TextRange::new(12, 20),
    };
    let dictionary_type = checker.dictionary_type(&constraint);
    WantedConstraint {
        class_id,
        arguments: constraint.arguments,
        dictionary_type,
        span: constraint.span,
        givens: Vec::new(),
        solution: None,
    }
}

fn unknown() -> InferType {
    InferType::Variable(900)
}

/// A second unsolved argument, so a goal can carry two of them without making
/// one the other's own tail — which the shared occurs check would, correctly,
/// reject.
fn other_unknown() -> InferType {
    InferType::Variable(901)
}

fn label(value: &str) -> InferType {
    InferType::TypeLevelString(value.to_owned())
}

fn int() -> InferType {
    InferType::Constructor(TypeConstructor::Int)
}

fn boolean() -> InferType {
    InferType::Constructor(TypeConstructor::Boolean)
}

/// The row `( label | () )`.
fn single(label: &str, field: InferType) -> InferType {
    InferType::RowExtend {
        label: label.to_owned(),
        ty: Box::new(field),
        tail: Box::new(InferType::RowEmpty),
    }
}

/// The row `( first | ( second | () ) )`, in cons order rather than sorted, so a
/// case can tell canonicalisation from a copy.
fn row(first: (&str, InferType), second: (&str, InferType)) -> InferType {
    InferType::RowExtend {
        label: first.0.to_owned(),
        ty: Box::new(first.1),
        tail: Box::new(single(second.0, second.1)),
    }
}

/// The arguments a solved obligation then has, read through the shared
/// substitution so a case can see the binding the rule recorded.
fn decided_arguments(checker: &mut Checker, goal: &WantedConstraint) -> Vec<InferType> {
    let PrimitiveDispatch::Solved(WantedSolution::Primitive { arguments }) =
        checker.solve_primitive(goal, SolveDepth::new())
    else {
        panic!("the rule must decide this goal");
    };
    arguments
}

/// A known label builds the extension the relation names, whatever the tail is.
/// Official `solveRowCons` does not read the tail, so an unsolved tail is
/// decided rather than declined.
#[test]
fn a_known_label_builds_the_extension() {
    let mut checker = checker();
    let tail = unknown();
    let row_argument = other_unknown();
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_CONS,
        vec![label("a"), int(), tail.clone(), row_argument.clone()],
    );

    assert_eq!(
        decided_arguments(&mut checker, &goal),
        vec![
            label("a"),
            int(),
            tail.clone(),
            single_with("a", int(), tail.clone()),
        ],
        "the decided row is the extension of the tail by the known label"
    );
    assert_eq!(
        checker.resolve_type(row_argument),
        single_with("a", int(), tail),
        "the binding went through the shared substitution, so the constraint's \
         own row argument now is what the rule decided"
    );
    assert!(checker.state.errors.is_empty());
}

/// `( label | tail )` for a tail that is not the empty row, which is the shape
/// `Cons` builds and the only one whose tail `row_from_fields` never produces.
fn single_with(label: &str, field: InferType, tail: InferType) -> InferType {
    InferType::RowExtend {
        label: label.to_owned(),
        ty: Box::new(field),
        tail: Box::new(tail),
    }
}

/// An unknown label is not an obligation this relation can answer: there is no
/// reading of the arguments that names the extension, so the rule declines and
/// leaves nothing behind.
#[test]
fn an_unknown_label_declines_and_leaves_no_state_behind() {
    let mut checker = checker();
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_CONS,
        vec![unknown(), int(), unknown(), unknown()],
    );
    let before = checker.state.substitutions.clone();

    assert!(matches!(
        checker.solve_primitive(&goal, SolveDepth::new()),
        PrimitiveDispatch::None
    ));
    assert_eq!(checker.state.substitutions, before);
    assert!(checker.state.errors.is_empty());
}

/// A closed row is canonicalised: labels ascending, each once. The rule reads the
/// row through the normalizer and rebuilds it, so the order the row was written
/// in is not part of what `Nub` decides.
#[test]
fn a_closed_row_is_canonicalized() {
    let mut checker = checker();
    let nubbed = unknown();
    let written = row(("b", boolean()), ("a", int()));
    let canonical = row(("a", int()), ("b", boolean()));
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_NUB,
        vec![written.clone(), nubbed.clone()],
    );

    assert_eq!(
        decided_arguments(&mut checker, &goal),
        vec![written, canonical.clone()],
        "a nubbed row is the same row in ascending label order"
    );
    assert_eq!(checker.resolve_type(nubbed), canonical);
    assert!(checker.state.errors.is_empty());
}

/// A label that occurs twice is this relation's own duplicate case, and official
/// keeps the first occurrence after a stable sort: the outer extension wins. A
/// map keyed by label would keep the inner one and throw away the order the row
/// was written in, which is the reading the design rejects.
#[test]
fn a_duplicate_label_keeps_the_outer_extension() {
    let mut checker = checker();
    let nubbed = unknown();
    let duplicated = single_with("a", int(), single("a", boolean()));
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_NUB,
        vec![duplicated.clone(), nubbed.clone()],
    );

    assert_eq!(
        decided_arguments(&mut checker, &goal),
        vec![duplicated, single("a", int())],
        "official's stable sort plus `nubBy` keeps the outer extension"
    );
    assert_eq!(checker.resolve_type(nubbed), single("a", int()));
    assert!(checker.state.errors.is_empty());
}

/// An open row has no nubbed form the rule can state: the answer depends on the
/// labels inside the tail, so the rule declines rather than deciding part of it.
#[test]
fn an_open_row_declines() {
    for class_id in [hir::TypeId::PRIM_ROW_NUB, hir::TypeId::PRIM_ROW_TO_LIST] {
        let mut checker = checker();
        let open = single_with("a", int(), unknown());
        let goal = goal(&mut checker, class_id, vec![open, unknown()]);
        let before = checker.state.substitutions.clone();

        assert!(
            matches!(
                checker.solve_primitive(&goal, SolveDepth::new()),
                PrimitiveDispatch::None
            ),
            "{class_id:?} must decline an open row rather than guess its tail"
        );
        assert_eq!(checker.state.substitutions, before);
        assert!(checker.state.errors.is_empty());
    }
}

/// A closed row becomes the `RowList` spine of `RowList.Cons` over
/// `RowList.Nil`, in the same ascending label order `Nub` uses, with the labels
/// as type-level strings.
#[test]
fn a_closed_row_becomes_a_row_list() {
    let mut checker = checker();
    let list = unknown();
    let written = row(("b", boolean()), ("a", int()));
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_TO_LIST,
        vec![written.clone(), list.clone()],
    );
    let expected = super::row_list_cons(
        "a",
        int(),
        super::row_list_cons("b", boolean(), super::row_list_nil()),
    );

    assert_eq!(
        decided_arguments(&mut checker, &goal),
        vec![written, expected.clone()],
        "the list names the row's labels in ascending order and ends at `Nil`"
    );
    assert_eq!(checker.resolve_type(list), expected);
    assert!(checker.state.errors.is_empty());
}

/// A decided row that contradicts a row already known to be something else cannot
/// hold, and the diagnostic is the shared unifier's rather than the rule's: it is
/// `TypesDoNotUnify` between the row the rule decided and the row the obligation
/// fixed, which is the code `purs` raises at the same step.
#[test]
fn a_decided_row_that_does_not_unify_is_reported() {
    let mut checker = checker();
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_NUB,
        vec![single("a", int()), single("b", boolean())],
    );

    assert!(matches!(
        checker.solve_primitive(&goal, SolveDepth::new()),
        PrimitiveDispatch::Reported
    ));
    assert_eq!(checker.state.errors.len(), 1);
    let error = &checker.state.errors[0];
    assert_eq!(error.kind, TypeCheckErrorKind::TypeMismatch);
    assert_eq!(error.error_code(), Some("TypesDoNotUnify"));
    assert_eq!(error.span, goal.span);
    assert!(
        !error.message().contains("does not hold"),
        "the rule no longer restates the unifier's own diagnostic: {}",
        error.message()
    );
}

/// A decided extension that contradicts the wanted row while the tail is still
/// unknown is still a contradiction: the wanted row and the label are both
/// determined, so the shared unifier finds the disagreement and reports it. This
/// is the case that used to be refused as a `Failed` — the rule bound nothing, so
/// the framework read it as no progress and the obligation reached instance search,
/// where it was reported as a missing instance instead. `purs` reports
/// `TypesDoNotUnify` here.
#[test]
fn a_contradiction_is_reported_while_the_tail_is_unknown() {
    let mut checker = checker();
    let goal = goal(
        &mut checker,
        hir::TypeId::PRIM_ROW_CONS,
        vec![label("a"), int(), unknown(), single("b", int())],
    );

    assert!(matches!(
        checker.solve_primitive(&goal, SolveDepth::new()),
        PrimitiveDispatch::Reported
    ));
    assert_eq!(checker.state.errors.len(), 1);
    let error = &checker.state.errors[0];
    assert_eq!(error.kind, TypeCheckErrorKind::TypeMismatch);
    assert_eq!(error.error_code(), Some("TypesDoNotUnify"));
    assert_eq!(
        error.span, goal.span,
        "the diagnostic keeps the obligation's own range"
    );
}
