//! The harness both `Compare` rule modules' cases share: a checker over an empty
//! module, so a rule is consulted directly on a goal built by hand rather than
//! through inference, and the argument shapes those goals are written in.

use super::*;
pub(super) use crate::typecheck::classes::SolveDepth;
pub(super) use crate::typecheck::prim::PrimitiveDispatch;
use psrs_span::SourceFile;

/// A checker over an empty module.
pub(super) fn checker() -> Checker {
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
pub(super) fn goal(
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

/// A rigid variable, which is what a signature's `forall` binds and what official
/// `Skolem` is.
pub(super) fn rigid(checker: &mut Checker) -> InferType {
    let id = match checker.fresh() {
        InferType::Variable(id) => id,
        _ => unreachable!("a fresh unknown is a variable"),
    };
    assert!(checker.state.rigid.insert(id));
    InferType::Variable(id)
}

/// A flexible unknown, which official `TUnknown` is.
pub(super) fn unknown(checker: &mut Checker) -> InferType {
    checker.fresh()
}

pub(super) fn int(value: i64) -> InferType {
    InferType::TypeLevelInt(value)
}

pub(super) fn symbol(value: &str) -> InferType {
    InferType::TypeLevelString(value.to_owned())
}

pub(super) fn ordering(which: Ordering) -> InferType {
    which.as_type()
}

/// A `Prim.Int.Compare` given in scope, as a declaration's dictionary parameter
/// would be.
pub(super) fn given(arguments: Vec<InferType>) -> (ClassConstraint, WantedSolution) {
    (
        ClassConstraint {
            class_id: hir::TypeId::PRIM_INT_COMPARE,
            arguments,
            span: TextRange::new(0, 8),
        },
        WantedSolution::Given(LocalId(0)),
    )
}

/// The arguments a solved obligation then has, read through the shared
/// substitution so a case sees the binding the rule recorded.
pub(super) fn decided_arguments(checker: &mut Checker, goal: &WantedConstraint) -> Vec<InferType> {
    let PrimitiveDispatch::Solved(WantedSolution::Primitive { arguments }) =
        checker.solve_primitive(goal, SolveDepth::new())
    else {
        panic!("the rule must decide this goal");
    };
    arguments
}
