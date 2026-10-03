//! The `Prim.Int` rules: `Add`, `Mul`, and `ToString`.
//!
//! All three are relations whose dictionaries are empty and erase, so what a
//! rule produces is a decision about type-level integers rather than a value.
//! `Add` is bidirectional over its arguments and the other two are not, and
//! that difference is the whole of their semantics:
//!
//! - `Add` decides forwards from two addends and backwards from an addend and
//!   the sum, which is what official `addInts` does and what its three fundeps
//!   state.
//! - `Mul` decides forwards only. It has one fundep, `left right -> product`,
//!   and neither official `solveIntMul` nor this rule recovers a factor from a
//!   known product.
//! - `ToString` decides forwards only, from a known integer.
//!
//! A decided argument is *stated* in the relation's dictionary rather than
//! assigned to the wanted argument, and the framework unifies a dictionary's own
//! arguments against the goal's — one operation, through the one substitution
//! every other part of inference reads. That is also what makes the diagnostic for
//! a disagreement the right one: official solving unifies a rule's decided
//! arguments against the arguments the goal wanted, so `IntToString1.purs` is
//! rejected by ordinary type equality under `TypesDoNotUnify` rather than by a
//! rule that inspected the wanted string, and a rigid argument in the decided
//! position is rejected by the same binder as the signature mismatch official's
//! `unifyTypes` makes of a skolem. For the same reason none of these rules
//! returns `Failed`: an answer that contradicts the goal is not the rule's to
//! notice, and a `NoInstanceFound` the suite does not expect would be a worse
//! guess than declining.
//!
//! The arguments arrive as ordinary `InferType` values through `PrimitiveArgs`,
//! so a type-level integer is read as [`InferType::TypeLevelInt`] off the shared
//! spine and a type-level string as [`InferType::TypeLevelString`]. No rule here
//! reads source syntax, carries a kind table, or invents a representation.

use super::{EvidenceClass, PrimitiveArgs, PrimitiveEvidence, PrimitiveOutcome, PrimitiveRule};
use crate::typecheck::*;

/// `Prim.Int.Add`'s entry in the rule table.
pub(in crate::typecheck) const ADD: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_INT_ADD,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 3,
    solve: solve_add,
};

/// `Prim.Int.Mul`'s entry in the rule table.
pub(in crate::typecheck) const MUL: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_INT_MUL,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 3,
    solve: solve_mul,
};

/// `Prim.Int.ToString`'s entry in the rule table.
pub(in crate::typecheck) const TO_STRING: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_INT_TO_STRING,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 2,
    solve: solve_to_string,
};

/// The `Prim.Int` rules, gathered so the table below registers this family as
/// one entry rather than three that a later revision has to keep in step.
pub(in crate::typecheck) const RULES: [PrimitiveRule; 3] = [ADD, MUL, TO_STRING];

/// The position of `Add`'s left addend and `Mul`'s left factor.
const LEFT: usize = 0;
/// The position of `Add`'s right addend, `Mul`'s right factor, and `ToString`'s
/// string.
const RIGHT: usize = 1;
/// The position of `Add`'s sum and `Mul`'s product.
const PRODUCT: usize = 2;

/// The integer an argument carries when it is a type-level literal.
///
/// A rigid variable is not one. Official solving treats a signature's variable
/// exactly this way: `printIntToString` matches `TypeLevelInt` and nothing else,
/// so `ToString t "a"` with a rigid `t` is left to instance search. Reading a
/// literal is therefore the whole of what this family knows about its arguments,
/// and an argument in any other shape is an argument the rule has no answer for.
fn literal_int(argument: &InferType) -> Option<i64> {
    match argument {
        InferType::TypeLevelInt(value) => Some(*value),
        _ => None,
    }
}

/// Whether `left right sum` is a relation this rule can decide, and which
/// position it decides.
///
/// The three directions are official `addInts`'s three arms, in its order, so a
/// goal whose arguments admit more than one reading gets the same one `purs`
/// gives it:
///
/// - two known addends give the sum;
/// - a known left addend and a known sum give the right addend;
/// - a known right addend and a known sum give the left addend.
///
/// Two or fewer known arguments decide nothing, and the rule declines: an
/// obligation with nothing to add is not one this relation can answer, and
/// reporting it as impossible while an argument is unknown is the conflation
/// `Undecided` and `Failed` exist to prevent.
///
/// A sum that leaves `i64` also declines. Official solves over `Integer` and has
/// no such bound, but a type-level `Int` in this compiler's shared model is an
/// `i64`, so a relation the model cannot state is undecided rather than a
/// wrapped, wrong answer.
fn add_decides(left: &InferType, right: &InferType, sum: &InferType) -> Option<(usize, i64)> {
    match (literal_int(left), literal_int(right), literal_int(sum)) {
        (Some(left), Some(right), _) => Some((PRODUCT, left.checked_add(right)?)),
        (Some(left), _, Some(sum)) => Some((RIGHT, sum.checked_sub(left)?)),
        (_, Some(right), Some(sum)) => Some((LEFT, sum.checked_sub(right)?)),
        _ => None,
    }
}

/// Decides `Add left right sum`.
fn solve_add(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [left, right, sum] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let Some((position, value)) = add_decides(left, right, sum) else {
        return PrimitiveOutcome::Undecided;
    };
    decide(&arguments, position, InferType::TypeLevelInt(value))
}

/// Decides `Mul left right product`.
///
/// Forwards only, as official `solveIntMul` is: it reads the two factors and
/// never a product. A goal with a known product and an unknown factor therefore
/// declines rather than searching for a factorisation, which is what the single
/// fundep `left right -> product` says there is to determine.
fn solve_mul(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [left, right, _] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let (Some(left), Some(right)) = (literal_int(left), literal_int(right)) else {
        return PrimitiveOutcome::Undecided;
    };
    let Some(product) = left.checked_mul(right) else {
        return PrimitiveOutcome::Undecided;
    };
    decide(&arguments, PRODUCT, InferType::TypeLevelInt(product))
}

/// Decides `ToString int string`.
///
/// A known integer determines the string through its decimal spelling, which for
/// a negative value is official `show`'s leading `-`. The result is a
/// `TypeLevelString` on the shared spine, so its payload is Unicode scalar
/// values per DEC-16 and the digits here are ASCII.
fn solve_to_string(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [int, _string] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let Some(value) = literal_int(int) else {
        return PrimitiveOutcome::Undecided;
    };
    decide(
        &arguments,
        RIGHT,
        InferType::TypeLevelString(value.to_string()),
    )
}

/// Puts the literal a direction decided at the position it determines, and
/// returns the relation's evidence.
///
/// The decision is *stated*, not applied. This is official's dictionary, whose
/// `tcdInstanceTypes` carry the decided literal at the position the rule decides
/// and the goal's own argument everywhere else, and the framework unifies those
/// against the goal's arguments through [`Checker::unify`]. That is what binds the
/// obligation when the argument is still unknown, so the constraint's arguments
/// become what the rule decided through the one substitution every other part of
/// inference reads, and what reports an argument that disagrees with the decision
/// under `TypesDoNotUnify`.
fn decide(arguments: &[InferType], position: usize, value: InferType) -> PrimitiveOutcome {
    if position >= arguments.len() {
        return PrimitiveOutcome::Undecided;
    }
    let mut decided = arguments.to_vec();
    decided[position] = value;
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary { arguments: decided },
        deferred: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
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

    /// A wanted obligation over `arguments`, with the range every diagnostic
    /// about it and every piece of evidence it produces carries.
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

    fn int(value: i64) -> InferType {
        InferType::TypeLevelInt(value)
    }

    fn unknown() -> InferType {
        InferType::Variable(900)
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

    #[test]
    fn a_negative_integer_decides_its_own_string() {
        let mut checker = checker();
        let string = unknown();
        let goal = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_TO_STRING,
            vec![int(-1), string],
        );

        assert_eq!(
            decided_arguments(&mut checker, &goal),
            vec![int(-1), InferType::TypeLevelString("-1".into())],
            "a negative type-level integer is a value like any other, and its \
             spelling is the one `show` gives"
        );
        assert!(checker.state.errors.is_empty());
    }

    #[test]
    fn a_decided_argument_is_bound_through_the_shared_substitution() {
        let mut checker = checker();
        let sum = unknown();
        let goal = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_ADD,
            vec![int(4), int(5), sum.clone()],
        );

        let arguments = decided_arguments(&mut checker, &goal);
        assert_eq!(checker.resolve_type(sum), int(9));
        assert_eq!(
            arguments,
            vec![int(4), int(5), int(9)],
            "the evidence records the arguments the constraint now has"
        );
    }

    /// Official solving has arbitrary-precision integers, so it never reaches
    /// this bound. A type-level `Int` here is an `i64`, and a sum outside it is
    /// a relation the shared model cannot state: the rule declines rather than
    /// wrapping into a different, wrong answer.
    #[test]
    fn an_add_that_would_leave_the_representable_range_declines() {
        let mut checker = checker();
        let sum = unknown();
        let goal = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_ADD,
            vec![int(i64::MAX), int(1), sum],
        );
        let before = checker.state.substitutions.clone();

        assert!(
            matches!(
                checker.solve_primitive(&goal, SolveDepth::new()),
                PrimitiveDispatch::None
            ),
            "an unrepresentable sum is undecided, not a wrapped answer"
        );
        assert_eq!(checker.state.substitutions, before);
        assert!(checker.state.errors.is_empty());
    }

    #[test]
    fn a_backwards_add_that_would_leave_the_representable_range_declines() {
        let mut checker = checker();
        let right = unknown();
        let goal = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_ADD,
            vec![int(1), right, int(i64::MIN)],
        );
        let before = checker.state.substitutions.clone();

        assert!(matches!(
            checker.solve_primitive(&goal, SolveDepth::new()),
            PrimitiveDispatch::None
        ));
        assert_eq!(checker.state.substitutions, before);
        assert!(checker.state.errors.is_empty());
    }

    #[test]
    fn a_mul_that_would_leave_the_representable_range_declines() {
        let mut checker = checker();
        let product = unknown();
        let goal = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_MUL,
            vec![int(i64::MAX), int(2), product],
        );
        let before = checker.state.substitutions.clone();

        assert!(matches!(
            checker.solve_primitive(&goal, SolveDepth::new()),
            PrimitiveDispatch::None
        ));
        assert_eq!(checker.state.substitutions, before);
        assert!(checker.state.errors.is_empty());
    }

    /// A goal with no pair of literals to read decides nothing. It continues into
    /// the ordinary paths with no substitution, level, kind, or diagnostic
    /// behind, which is what makes declining a rule rather than a failure.
    #[test]
    fn a_goal_with_no_known_pair_declines_and_leaves_no_state_behind() {
        for (class_id, arity) in [
            (hir::TypeId::PRIM_INT_ADD, 3),
            (hir::TypeId::PRIM_INT_MUL, 3),
            (hir::TypeId::PRIM_INT_TO_STRING, 2),
        ] {
            let mut checker = checker();
            let goal = goal(
                &mut checker,
                class_id,
                (0..arity).map(|_| unknown()).collect(),
            );
            let before = checker.state.substitutions.clone();

            assert!(
                matches!(
                    checker.solve_primitive(&goal, SolveDepth::new()),
                    PrimitiveDispatch::None
                ),
                "{class_id:?} must decline a goal it cannot decide"
            );
            assert_eq!(checker.state.substitutions, before);
            assert!(checker.state.errors.is_empty());
        }
    }

    /// `ToString` decides forwards only, and `Mul` with a known product but an
    /// unknown factor declines rather than searching for a factorisation: a
    /// product is not evidence of its factors.
    #[test]
    fn mul_and_to_string_do_not_decide_backwards() {
        let mut checker = checker();
        let product_goal = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_MUL,
            vec![unknown(), int(3), int(12)],
        );
        assert!(matches!(
            checker.solve_primitive(&product_goal, SolveDepth::new()),
            PrimitiveDispatch::None
        ));

        let string_goal = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_TO_STRING,
            vec![unknown(), InferType::TypeLevelString("3".into())],
        );
        assert!(matches!(
            checker.solve_primitive(&string_goal, SolveDepth::new()),
            PrimitiveDispatch::None
        ));
        assert!(checker.state.errors.is_empty());
    }
}
