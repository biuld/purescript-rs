//! `Prim.Symbol.Compare` and `Prim.Int.Compare`: the two relations that decide
//! an `Ordering`.
//!
//! Both have the same shape — two operands and an `Ordering` — and both *state*
//! the ordering they decide in the relation's dictionary rather than assigning it
//! to the wanted argument; the framework unifies a dictionary's own arguments
//! against the goal's, so a goal whose ordering disagrees with the one the rule
//! decided is rejected by ordinary type equality under `TypesDoNotUnify`. That is
//! official behaviour rather than a choice: `TypeChecker.Entailment` unifies a
//! rule's decided arguments against the goal's arguments for every dictionary it
//! builds. Neither rule returns `Failed`, for the same reason the `Prim.Int` rules
//! do not: an answer that contradicts the goal is the framework's to find, not
//! the rule's to report.
//!
//! What differs is how much each relation decides.
//!
//! - `Symbol.Compare` reads two known type-level strings and compares them in
//!   scalar-value order, which is what official `solveSymbolCompare` does with
//!   Haskell's `compare` on the two `Text` payloads. One known operand decides
//!   nothing.
//! - `Int.Compare` is not only about literals. Official `solveIntCompare` reads
//!   two known integers first, and otherwise closes a *relation* over the
//!   `Compare` dictionaries in scope, which [`relation`] implements. Given
//!   `Compare a b EQ` and `Compare b c LT`, the path `a -> b -> c` decides
//!   `Compare a c LT`. `CompareInt3.purs` is that case, and a rule that only
//!   compared literals would decline it.
//!
//! The `Ordering` itself is not special here. `LT`, `EQ`, and `GT` are ordinary
//! nominal constructors in the registry, so a decision is an ordinary
//! `InferType::Constructor` built from the shared `TypeId`s, and the arguments
//! are read through [`PrimitiveArgs`] and the shared substitution like every
//! other rule's.

mod relation;
#[cfg(test)]
mod tests;

use super::{EvidenceClass, PrimitiveArgs, PrimitiveEvidence, PrimitiveOutcome, PrimitiveRule};
use crate::typecheck::*;
use relation::close_relation;

/// The two `Compare` relations this module decides, as the rule table records
/// them.
pub(in crate::typecheck) const RULES: [PrimitiveRule; 2] = [SYMBOL_COMPARE, INT_COMPARE];

/// `Prim.Symbol.Compare`'s entry in the rule table.
const SYMBOL_COMPARE: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_SYMBOL_COMPARE,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 3,
    solve: solve_symbol_compare,
};

/// `Prim.Int.Compare`'s entry in the rule table.
const INT_COMPARE: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_INT_COMPARE,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 3,
    solve: solve_int_compare,
};

/// The position of the `Ordering` both members decide. It is the third argument
/// of each declaration, and it is the only one either rule ever binds.
const ORDERING: usize = 2;

/// The three answers a comparison decides.
///
/// This is official `Prim.Ordering`'s three nominal constructors, read through
/// the identities the registry declares rather than through a private node: a
/// decision is an ordinary type that unification, display, and THIR already
/// understand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ordering {
    Lt,
    Eq,
    Gt,
}

impl Ordering {
    /// The ordering of two type-level symbols.
    ///
    /// A `Symbol` is a sequence of Unicode scalar values (DEC-16), and its
    /// payload is such a sequence, so this is scalar-value order: `char`'s own
    /// `Ord`, element by element, with the shorter sequence smaller when one is a
    /// prefix of the other. That is official `solveSymbolCompare`, which calls
    /// `compare` on the two decoded `Text` payloads.
    fn of_symbols(left: &str, right: &str) -> Ordering {
        of_comparison(left.cmp(right))
    }

    /// The ordering of two type-level integers.
    fn of_integers(left: i64, right: i64) -> Ordering {
        of_comparison(left.cmp(&right))
    }

    /// The decided ordering as the nominal type it is.
    fn as_type(self) -> InferType {
        InferType::Constructor(TypeConstructor::User(match self {
            Ordering::Lt => hir::TypeId::PRIM_ORDERING_LT,
            Ordering::Eq => hir::TypeId::PRIM_ORDERING_EQ,
            Ordering::Gt => hir::TypeId::PRIM_ORDERING_GT,
        }))
    }
}

/// One `compare` result, read once so the two rules cannot spell the mapping
/// differently.
fn of_comparison(comparison: std::cmp::Ordering) -> Ordering {
    match comparison {
        std::cmp::Ordering::Less => Ordering::Lt,
        std::cmp::Ordering::Equal => Ordering::Eq,
        std::cmp::Ordering::Greater => Ordering::Gt,
    }
}

/// The `Ordering` `ty` is, when it is one of the three nominal constructors.
/// Anything else — an unsolved variable, another type — is a decision nobody has
/// made yet, which is what official `mkRelation` declining means.
pub(super) fn known_ordering(ty: &InferType) -> Option<Ordering> {
    let InferType::Constructor(TypeConstructor::User(constructor)) = ty else {
        return None;
    };
    match *constructor {
        hir::TypeId::PRIM_ORDERING_LT => Some(Ordering::Lt),
        hir::TypeId::PRIM_ORDERING_EQ => Some(Ordering::Eq),
        hir::TypeId::PRIM_ORDERING_GT => Some(Ordering::Gt),
        _ => None,
    }
}

/// The type-level string `argument` is, or `None` when it is not one.
fn known_symbol(argument: &InferType) -> Option<&str> {
    match argument {
        InferType::TypeLevelString(value) => Some(value.as_str()),
        _ => None,
    }
}

/// The integer an argument carries when it is a type-level literal. A rigid
/// variable is not one, exactly as in official `printIntToString`.
pub(super) fn literal_int(argument: &InferType) -> Option<i64> {
    match argument {
        InferType::TypeLevelInt(value) => Some(*value),
        _ => None,
    }
}

/// Decides `Compare left right ordering` over two type-level symbols.
///
/// Official `solveSymbolCompare` has exactly one reading — two known symbols —
/// and one known symbol with one unknown operand decides nothing. It does not
/// read the wanted ordering back either: `Symbol.Compare` decides forwards,
/// because the ordering is derived from the two symbols rather than recovered
/// from it.
fn solve_symbol_compare(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [left, right, _] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let (Some(left), Some(right)) = (known_symbol(left), known_symbol(right)) else {
        return PrimitiveOutcome::Undecided;
    };
    decided(&arguments, Ordering::of_symbols(left, right))
}

/// Decides `Compare left right ordering` over two type-level integers.
///
/// Official `solveIntCompare` has two readings, in this order, and an arm that
/// applies is never retried as a later one:
///
/// - two known integers decide by their value, and nothing in scope is read;
/// - otherwise the relation over the scope is closed, which decides from the
///   orderings the in-scope dictionaries carry and from the type-level literals
///   they mention.
///
/// A goal with two unknown operands and nothing in scope to relate them declines,
/// so it continues into instance search rather than being reported as
/// impossible.
fn solve_int_compare(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [left, right, _] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    if let (Some(left), Some(right)) = (literal_int(left), literal_int(right)) {
        return decided(&arguments, Ordering::of_integers(left, right));
    }
    match close_relation(checker, &arguments) {
        Some(ordering) => decided(&arguments, ordering),
        None => PrimitiveOutcome::Undecided,
    }
}

/// Puts the ordering the two rules decided at the position they decide, and
/// returns the relation's evidence.
///
/// The decision is *stated*, not applied. This is official's dictionary, whose
/// `tcdInstanceTypes` carry the decided `Ordering` at the position the rule
/// decides and the goal's own arguments everywhere else, and the framework
/// unifies those against the goal's through [`Checker::unify`]. That is what binds
/// the ordering argument when it is still unknown, so the evidence records the
/// arguments the obligation *then* has rather than the ones the rule intended, and
/// what rejects an ordering that disagrees with the decision under
/// `TypesDoNotUnify` — the shared step `Int.Add`'s sum and `Symbol.Cons`'s head
/// reach through the same way.
fn decided(arguments: &[InferType], ordering: Ordering) -> PrimitiveOutcome {
    if ORDERING >= arguments.len() {
        return PrimitiveOutcome::Undecided;
    }
    let mut decided = arguments.to_vec();
    decided[ORDERING] = ordering.as_type();
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary { arguments: decided },
        deferred: Vec::new(),
    }
}

#[cfg(test)]
mod rule_tests {
    use super::tests::*;
    use super::*;

    /// Both members read two known operands and compare them by value. Two
    /// symbols compare in scalar-value order, which is DEC-16's sequence of
    /// scalar values rather than a locale collation.
    #[test]
    fn two_known_symbols_decide_by_scalar_value() {
        for (left, right, expected) in [
            ("A", "B", Ordering::Lt),
            ("A", "A", Ordering::Eq),
            ("b", "A", Ordering::Gt),
            ("A", "AB", Ordering::Lt),
            ("", "A", Ordering::Lt),
            // U+007A is below U+00E9, so `"z" < "é"` even though a locale
            // collation of the two would not be in that order.
            ("z", "é", Ordering::Lt),
        ] {
            let mut checker = checker();
            let free = unknown(&mut checker);
            let goal = goal(
                &mut checker,
                hir::TypeId::PRIM_SYMBOL_COMPARE,
                vec![symbol(left), symbol(right), free],
            );

            assert_eq!(
                decided_arguments(&mut checker, &goal),
                vec![symbol(left), symbol(right), ordering(expected)],
                "official `solveSymbolCompare` compares {left:?} with {right:?} \
                 and reports {expected:?}"
            );
            assert!(checker.state.errors.is_empty());
        }
    }

    /// Two known integers decide by value, and the decided ordering is bound
    /// through the shared substitution rather than assigned to the argument.
    #[test]
    fn two_known_integers_decide_by_value() {
        let mut checker = checker();
        let decided = unknown(&mut checker);
        let goal = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_COMPARE,
            vec![int(1), int(2), decided.clone()],
        );

        assert_eq!(
            decided_arguments(&mut checker, &goal),
            vec![int(1), int(2), ordering(Ordering::Lt)]
        );
        assert_eq!(checker.resolve_type(decided), ordering(Ordering::Lt));
    }

    /// `Symbol.Compare` reads both operands. Official has one arm, so one known
    /// symbol decides nothing, and the obligation continues into instance search
    /// with no substitution, level, kind, or diagnostic behind.
    #[test]
    fn one_known_symbol_declines() {
        for arguments in [
            vec![
                symbol("A"),
                InferType::Variable(900),
                InferType::Variable(901),
            ],
            vec![
                InferType::Variable(900),
                symbol("A"),
                InferType::Variable(901),
            ],
            vec![
                InferType::Variable(900),
                InferType::Variable(901),
                InferType::Variable(902),
            ],
        ] {
            let mut checker = checker();
            let goal = goal(
                &mut checker,
                hir::TypeId::PRIM_SYMBOL_COMPARE,
                arguments.clone(),
            );
            let before = checker.state.substitutions.clone();

            assert!(
                matches!(
                    checker.solve_primitive(&goal, SolveDepth::new()),
                    PrimitiveDispatch::None
                ),
                "one known symbol is not a comparison: {arguments:?}"
            );
            assert_eq!(checker.state.substitutions, before);
            assert!(checker.state.errors.is_empty());
        }
    }

    /// An ordering that disagrees with the decision is rejected by ordinary type
    /// equality, under `TypesDoNotUnify`, which is the code official solving
    /// raises for its own decided argument. The framework's own check is what
    /// finds it: neither rule inspects the wanted ordering, and neither returns
    /// `Failed`.
    #[test]
    fn a_wanted_ordering_that_disagrees_is_rejected_by_ordinary_equality() {
        for (class_id, arguments) in [
            (
                hir::TypeId::PRIM_SYMBOL_COMPARE,
                vec![symbol("B"), symbol("A"), ordering(Ordering::Lt)],
            ),
            (
                hir::TypeId::PRIM_INT_COMPARE,
                vec![int(2), int(1), ordering(Ordering::Lt)],
            ),
        ] {
            let mut checker = checker();
            let goal = goal(&mut checker, class_id, arguments);

            assert!(matches!(
                checker.solve_primitive(&goal, SolveDepth::new()),
                PrimitiveDispatch::Reported
            ));
            let errors = &checker.state.errors;
            assert!(
                errors
                    .iter()
                    .any(|error| error.kind == TypeCheckErrorKind::TypeMismatch),
                "{class_id:?} must report the disagreement by ordinary equality: {errors:?}"
            );
            assert!(errors.iter().all(|error| error.span == goal.span));
        }
    }
}
