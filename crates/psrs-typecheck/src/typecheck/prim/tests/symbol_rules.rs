//! `Prim.Symbol.Append` and `Prim.Symbol.Cons` as the rule table dispatches
//! them.
//!
//! The cases here are the readings each relation has, each decline, each definite
//! failure, and each contradiction, driven through `solve_primitive` so that the
//! framework's own acceptance is part of what is under test: a decision stated for
//! an unknown argument is bound by the framework's unification, a decision that
//! contradicts the goal is rejected by that same unification, and an outcome the
//! framework refuses is not the outcome the rule returned.
//!
//! Every case is pinned against `purs` 0.15.16. Both relations are declared by
//! the compiler itself, so a case needs nothing but `Prim` to run.

use super::checker as new_checker;
use super::*;

/// Dispatches one obligation through the rule table.
fn dispatch(
    checker: &mut Checker,
    class_id: hir::TypeId,
    arguments: Vec<InferType>,
) -> PrimitiveDispatch {
    let constraint = obligation(checker, class_id, arguments);
    checker.solve_primitive(&constraint, SolveDepth::new())
}

/// The symbols the constraint holds after the rule decided them.
fn solved(checker: &mut Checker, class_id: hir::TypeId, arguments: Vec<InferType>) -> Vec<String> {
    let PrimitiveDispatch::Solved(WantedSolution::Primitive { arguments }) =
        dispatch(checker, class_id, arguments)
    else {
        panic!("the rule decides this relation");
    };
    arguments
        .iter()
        .map(|argument| match argument {
            InferType::TypeLevelString(symbol) => symbol.clone(),
            other => panic!("{other:?} is not the decided symbol"),
        })
        .collect()
}

/// The shared unifier's reason for a rejection, checked under `code`.
///
/// A relation's rule states what it decided and the framework unifies that against
/// the goal, so the reason is the unifier's own `type mismatch: expected <decided>,
/// found <wanted>` rather than a restatement naming the relation.
/// `crates/psrs-driver/tests/prim_symbol.rs` pins the same reasons from a checked
/// program.
fn reason(
    checker: &mut Checker,
    class_id: hir::TypeId,
    code: &str,
    arguments: Vec<InferType>,
) -> String {
    assert!(
        matches!(
            dispatch(checker, class_id, arguments),
            PrimitiveDispatch::Reported
        ),
        "the rule reports this obligation"
    );
    let error = checker
        .state
        .errors
        .last()
        .expect("a reported obligation keeps its diagnostic");
    assert_eq!(
        error.error_code(),
        Some(code),
        "the official code the rule reports under"
    );
    error.message().to_owned()
}

/// The type-level string an argument carries.
fn symbol(value: &str) -> InferType {
    InferType::TypeLevelString(value.to_owned())
}

/// A fresh variable marked rigid, which is what a signature's variable is: as
/// determined as a literal for the framework, and never assigned by a rule.
fn rigid(checker: &mut Checker) -> InferType {
    let ty = checker.fresh();
    if let InferType::Variable(variable) = &ty {
        checker.state.rigid.insert(*variable);
    }
    ty
}

const APPEND: hir::TypeId = hir::TypeId::PRIM_SYMBOL_APPEND;
const CONS: hir::TypeId = hir::TypeId::PRIM_SYMBOL_CONS;

/// `purs` raises the mismatch of its own decided argument when a decided symbol
/// does not unify, and reports no instance when no reading of the arguments can
/// produce one. Both codes come from the framework here: the mismatch from the
/// shared unifier, the missing instance from instance search.
const MISMATCH: &str = "TypesDoNotUnify";

#[test]
fn both_symbol_relations_are_dispatched_as_relations_with_three_arguments() {
    assert_eq!(primitive_rule(APPEND).map(|rule| rule.arity), Some(3));
    assert_eq!(primitive_rule(CONS).map(|rule| rule.arity), Some(3));
    for class_id in [APPEND, CONS] {
        assert!(
            primitive_rule_precedes_givens(class_id),
            "a relation decides its type-level fact before a dictionary can mask it"
        );
    }
}

/// Both inputs known: the third is their concatenation. `purs` accepts
/// `Append "a" "b" "ab"` and rejects `Append "a" "b" "ba"` as `TypesDoNotUnify`
/// on the symbol it decided.
#[test]
fn append_concatenates_two_known_symbols() {
    let mut checker = new_checker();
    assert_eq!(
        solved(
            &mut checker,
            APPEND,
            vec![symbol("a"), symbol("b"), symbol("ab")]
        ),
        vec!["a", "b", "ab"]
    );
    assert!(checker.state.errors.is_empty());

    let mut checker = new_checker();
    assert_eq!(
        reason(
            &mut checker,
            APPEND,
            MISMATCH,
            vec![symbol("a"), symbol("b"), symbol("ba")]
        ),
        "type mismatch: expected \"ab\", found \"ba\""
    );
}

/// The left symbol and the appended symbol known: the right symbol is what
/// follows the left one. `purs` accepts `Append "a" "b" "ab"`.
#[test]
fn append_strips_a_known_prefix() {
    let mut checker = new_checker();
    let right = checker.fresh();
    assert_eq!(
        solved(
            &mut checker,
            APPEND,
            vec![symbol("ab"), right, symbol("abc")]
        ),
        vec!["ab", "c", "abc"]
    );
    assert!(checker.state.errors.is_empty());
}

/// The right symbol and the appended symbol known: the left symbol is what
/// precedes the right one. `purs` accepts `Append "ab" "c" "abc"`.
#[test]
fn append_strips_a_known_suffix() {
    let mut checker = new_checker();
    let left = checker.fresh();
    assert_eq!(
        solved(&mut checker, APPEND, vec![left, symbol("c"), symbol("abc")]),
        vec!["ab", "c", "abc"]
    );
    assert!(checker.state.errors.is_empty());
}

/// The reading the arguments' shape selects is the only one. A known left
/// symbol and a known appended symbol are read as a prefix and never as a
/// suffix, and a prefix that is not one declines rather than falling through to
/// the other reading. `purs` reports `NoInstanceFound` for `Append "b" s "abc"`
/// and for `Append "a" s "ba"`, which is what instance search reports here too.
#[test]
fn append_declines_when_the_selected_reading_does_not_decide() {
    // "b" is not a prefix of "abc"; the suffix reading would have answered "a".
    let mut checker = new_checker();
    let right = checker.fresh();
    assert!(matches!(
        dispatch(
            &mut checker,
            APPEND,
            vec![symbol("b"), right, symbol("abc")]
        ),
        PrimitiveDispatch::None
    ));
    assert!(checker.state.errors.is_empty());

    // "a" is not a prefix of "ba".
    let mut checker = new_checker();
    let right = checker.fresh();
    assert!(matches!(
        dispatch(&mut checker, APPEND, vec![symbol("a"), right, symbol("ba")]),
        PrimitiveDispatch::None
    ));
    assert!(checker.state.errors.is_empty());

    // "c" is a suffix of "abc", so this reading does decide the left symbol.
    let mut checker = new_checker();
    let left = checker.fresh();
    assert_eq!(
        solved(&mut checker, APPEND, vec![left, symbol("c"), symbol("abc")]),
        vec!["ab", "c", "abc"]
    );
}

/// No pair is known, so no reading applies and the obligation continues into
/// instance search. `purs` reports `NoInstanceFound` for `Append l r s`.
#[test]
fn append_declines_when_no_argument_is_known() {
    let mut checker = new_checker();
    let left = checker.fresh();
    let right = checker.fresh();
    let appended = checker.fresh();
    assert!(matches!(
        dispatch(&mut checker, APPEND, vec![left, right, appended]),
        PrimitiveDispatch::None
    ));
    assert!(checker.state.errors.is_empty());
}

/// A known head and a known tail join into the symbol. `purs` accepts
/// `Cons "a" "bc" "abc"`.
#[test]
fn cons_joins_a_one_scalar_head_and_a_tail() {
    let mut checker = new_checker();
    let whole = checker.fresh();
    assert_eq!(
        solved(&mut checker, CONS, vec![symbol("a"), symbol("bc"), whole]),
        vec!["a", "bc", "abc"]
    );
    assert!(checker.state.errors.is_empty());
}

/// A known symbol splits into its first scalar and the rest. `purs` accepts
/// `Cons "a" "bc" "abc"`.
#[test]
fn cons_splits_a_known_symbol() {
    let mut checker = new_checker();
    let head = checker.fresh();
    let tail = checker.fresh();
    assert_eq!(
        solved(&mut checker, CONS, vec![head, tail, symbol("abc")]),
        vec!["a", "bc", "abc"]
    );
    assert!(checker.state.errors.is_empty());
}

/// The splitting reading comes first, so a symbol that disagrees with a known
/// head or tail is a mismatch rather than a join. `purs` reports
/// `TypesDoNotUnify` for `Cons "ab" "c" "abc"` and for `Cons "a" "bc" "a"`,
/// having split the symbol and unified the halves with the wanted ones.
#[test]
fn cons_reports_a_split_that_contradicts_a_known_half() {
    let mut checker = new_checker();
    assert_eq!(
        reason(
            &mut checker,
            CONS,
            MISMATCH,
            vec![symbol("ab"), symbol("c"), symbol("abc")]
        ),
        "type mismatch: expected \"a\", found \"ab\""
    );

    let mut checker = new_checker();
    assert_eq!(
        reason(
            &mut checker,
            CONS,
            MISMATCH,
            vec![symbol("a"), symbol("bc"), symbol("a")]
        ),
        "type mismatch: expected \"\", found \"bc\""
    );
}

/// A split that decides one half and contradicts the other still fails, and the
/// half it bound is what the framework got the first half of the check from.
/// `purs` unifies the halves it computed with the wanted ones in the same order,
/// and rejects `Cons h "bc" "ab"` the same way.
#[test]
fn cons_reports_a_split_that_binds_the_head_and_contradicts_the_tail() {
    let mut checker = checker();
    let head = checker.fresh();
    assert_eq!(
        reason(
            &mut checker,
            CONS,
            MISMATCH,
            vec![head, symbol("bc"), symbol("ab")]
        ),
        "type mismatch: expected \"b\", found \"bc\""
    );
}

/// The same split, with the contradiction at the position the rule decided *first*
/// and the other half still open. The two halves are decided together, so a rule
/// that reported the failure itself had it refused as "not determined" — the
/// framework reads a `Failed` as impossible only once every argument is — and the
/// obligation reached instance search as a missing instance. `purs` unifies the
/// computed halves against the wanted ones in order, so it rejects
/// `Cons "ab" t "a"` at the head whatever `t` is.
#[test]
fn cons_reports_a_split_that_contradicts_the_head_while_the_tail_is_open() {
    let mut checker = checker();
    let tail = checker.fresh();
    assert_eq!(
        reason(
            &mut checker,
            CONS,
            MISMATCH,
            vec![symbol("ab"), tail, symbol("a")]
        ),
        "type mismatch: expected \"a\", found \"ab\""
    );
}

/// An empty symbol has no first scalar, so the splitting reading decides
/// nothing. `purs` reports `NoInstanceFound` for `Cons "a" "" ""`.
#[test]
fn cons_declines_on_an_empty_symbol() {
    let mut checker = new_checker();
    assert!(matches!(
        dispatch(
            &mut checker,
            CONS,
            vec![symbol("a"), symbol(""), symbol("")]
        ),
        PrimitiveDispatch::None
    ));
    assert!(checker.state.errors.is_empty());
}

/// A head that is not one scalar cannot produce a result, but the result is
/// still flexible and can be generalized with the residual relation. The rule
/// declines both cases, leaving retention to the ordinary wanted solver.
#[test]
fn a_head_that_is_not_one_scalar_is_refused_while_the_symbol_is_unknown() {
    let mut checker = new_checker();
    let whole = checker.fresh();
    assert!(matches!(
        dispatch(&mut checker, CONS, vec![symbol("ab"), symbol("c"), whole]),
        PrimitiveDispatch::None
    ));
    assert!(
        checker.state.errors.is_empty(),
        "a refused outcome leaves no diagnostic behind"
    );

    let mut checker = new_checker();
    let whole = checker.fresh();
    assert!(matches!(
        dispatch(&mut checker, CONS, vec![symbol(""), symbol("bc"), whole]),
        PrimitiveDispatch::None
    ));
    assert!(checker.state.errors.is_empty());
}

/// A rigid result is not evidence that the relation fails. The rule declines;
/// the required-solved boundary is responsible for reporting a missing instance.
#[test]
fn an_invalid_head_with_a_rigid_result_is_left_to_required_solved_search() {
    let mut checker = new_checker();
    let whole = rigid(&mut checker);
    assert!(matches!(
        dispatch(&mut checker, CONS, vec![symbol("ab"), symbol("c"), whole]),
        PrimitiveDispatch::None
    ));
    assert!(checker.state.errors.is_empty());
}

/// A symbol is a sequence of scalar values, not bytes, so both directions treat
/// one multi-byte scalar as one character.
#[test]
fn a_multi_byte_scalar_is_one_character() {
    let mut checker = new_checker();
    let whole = checker.fresh();
    assert_eq!(
        solved(
            &mut checker,
            CONS,
            vec![symbol("\u{1f600}"), symbol("x"), whole]
        ),
        vec!["\u{1f600}", "x", "\u{1f600}x"]
    );
    assert!(checker.state.errors.is_empty());

    let mut checker = new_checker();
    let head = checker.fresh();
    let tail = checker.fresh();
    assert_eq!(
        solved(&mut checker, CONS, vec![head, tail, symbol("a\u{1f600}bc")]),
        vec!["a", "\u{1f600}bc", "a\u{1f600}bc"]
    );
    assert!(checker.state.errors.is_empty());
}
