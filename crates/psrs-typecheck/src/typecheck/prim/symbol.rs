//! `Prim.Symbol.Append` and `Prim.Symbol.Cons`: the two relations whose
//! arguments are type-level strings, and the two rules that decide one from
//! others.
//!
//! Both relations are bidirectional, and both are decided by the same three
//! ordinary arguments on the shared spine: a rule reads them through
//! [`PrimitiveArgs`], decides a symbol from the ones it knows, and binds that
//! decision through the shared substitution. Nothing here parses source text or
//! carries a second reading of what a `Symbol` is — a type-level string is
//! [`InferType::TypeLevelString`], its payload is a sequence of Unicode scalar
//! values, and the member's declared kind is what makes reading the argument as
//! a string the right reading. `typecheck/kind.rs` owns that check.
//!
//! Official `TypeChecker/Entailment.hs` is the reference, and both rules follow
//! its arm order exactly, because the order is part of what each rule decides.
//! An arm is selected by the *shape* of the arguments, and an arm that matched is
//! never retried as a later one: a `Cons` whose symbol is known is decided by
//! splitting that symbol even when the head and the tail are known too, and an
//! `Append` whose left symbol and appended symbol are known is decided by
//! stripping a prefix even when stripping a suffix would have produced an
//! answer.
//!
//! The other thing the reference fixes is that an arm which cannot decide
//! returns nothing rather than trying the next reading: `Append` strips a prefix
//! only if the left symbol is a genuine prefix, and `Cons` joins only if the
//! head is exactly one scalar. Those are the declines and the one definite failure
//! below, and neither is ever answered with a guess.
//!
//! A decision is *stated* in the relation's dictionary rather than assigned to the
//! wanted argument, and the framework unifies a dictionary's own arguments against
//! the goal's — official `Entailment.hs:301`, in `super::verify`. So a split or a
//! concatenation that contradicts an argument the obligation already fixed is
//! found there and rejected under `TypesDoNotUnify`, and no rule here inspects the
//! goal to notice, restates, or re-report it. That includes a split whose
//! contradiction sits at a position another argument leaves unknown: the head and
//! the tail are decided together, so a rule that reported the failure itself would
//! have it refused as "not determined" and lose the contradiction, which is what
//! `Cons "ab" s "a"` with `s` still open used to do.

use super::{EvidenceClass, PrimitiveArgs, PrimitiveEvidence, PrimitiveOutcome, PrimitiveRule};
use crate::typecheck::*;

/// The two `Prim.Symbol` relations this module decides, as the rule table
/// records them.
pub(in crate::typecheck) const RULES: [PrimitiveRule; 2] = [APPEND, CONS];

/// `Append`'s entry in the rule table.
const APPEND: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_SYMBOL_APPEND,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 3,
    solve: solve_append,
};

/// `Cons`'s entry in the rule table.
const CONS: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::PRIM_SYMBOL_CONS,
    evidence: EvidenceClass::RuntimeDictionary,
    arity: 3,
    solve: solve_cons,
};

/// The argument positions of each member, in declaration order. A decision
/// carries the position it fills, which is what official's dictionary needs in
/// order to say where the rule's answer goes.
const APPEND_LEFT: usize = 0;
const APPEND_RIGHT: usize = 1;
const APPEND_APPENDED: usize = 2;
const CONS_HEAD: usize = 0;
const CONS_TAIL: usize = 1;
const CONS_SYMBOL: usize = 2;

/// One argument a rule decided: the position it fills and the symbol it is
/// decided to.
struct Decision {
    position: usize,
    symbol: String,
}

/// Whether `Append left right appended` holds for three type-level symbols.
///
/// The relation reads forwards and backwards, and official `appendSymbols`
/// chooses between the three readings by the shape of its arguments:
///
/// - both inputs known: the third is their concatenation;
/// - the left symbol and the appended symbol known: the right symbol is what
///   follows the left one, and only when the left one is a genuine prefix;
/// - the right symbol and the appended symbol known: the left symbol is what
///   precedes the right one, and only when the right one is a genuine suffix.
///
/// No reading applies when no pair is known, and none decides when its prefix or
/// suffix check fails, so the rule declines and the obligation continues into
/// instance search. Neither is a failure: a known left symbol that is not a
/// prefix of a known appended symbol says nothing about the right symbol until
/// the right symbol is known.
fn solve_append(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [left, right, appended] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let decisions = match (known_symbol(left), known_symbol(right)) {
        (Some(left), Some(right)) => vec![Decision {
            position: APPEND_APPENDED,
            symbol: String::from(left) + right,
        }],
        (Some(left), _) => {
            match known_symbol(appended).and_then(|appended| appended.strip_prefix(left)) {
                Some(suffix) => vec![Decision {
                    position: APPEND_RIGHT,
                    symbol: suffix.to_owned(),
                }],
                None => return PrimitiveOutcome::Undecided,
            }
        }
        (_, Some(right)) => {
            match known_symbol(appended).and_then(|appended| appended.strip_suffix(right)) {
                Some(prefix) => vec![Decision {
                    position: APPEND_LEFT,
                    symbol: prefix.to_owned(),
                }],
                None => return PrimitiveOutcome::Undecided,
            }
        }
        _ => return PrimitiveOutcome::Undecided,
    };
    decided(&arguments, decisions)
}

/// Whether `Cons head tail symbol` holds for three type-level symbols.
///
/// Official `consSymbol` reads a known symbol first and joins a head and a tail
/// only when the symbol is unknown:
///
/// - the symbol known: it splits into its first scalar and the rest;
/// - the head and the tail known: the symbol is their concatenation, and only
///   when the head is exactly one scalar, because `Cons` is a cons cell.
///
/// An empty symbol has no first scalar, so the splitting reading decides nothing
/// and the rule declines. A head that is empty or longer than one scalar also
/// declines when the result is open: official solving can generalize that result
/// together with the residual relation, and this rule has no dictionary to
/// produce until a reading decides the result.
fn solve_cons(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [head, tail, whole] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let decisions = match known_symbol(whole) {
        Some(whole) => {
            let mut scalars = whole.chars();
            match scalars.next() {
                Some(first) => vec![
                    Decision {
                        position: CONS_HEAD,
                        symbol: first.to_string(),
                    },
                    Decision {
                        position: CONS_TAIL,
                        symbol: scalars.as_str().to_owned(),
                    },
                ],
                None => return PrimitiveOutcome::Undecided,
            }
        }
        None => match (known_symbol(head), known_symbol(tail)) {
            (Some(head), Some(tail)) if head.chars().count() == 1 => vec![Decision {
                position: CONS_SYMBOL,
                symbol: String::from(head) + tail,
            }],
            (Some(_), Some(_)) => return PrimitiveOutcome::Undecided,
            _ => return PrimitiveOutcome::Undecided,
        },
    };
    decided(&arguments, decisions)
}

/// Puts each decision at the position it fills and returns the relation's
/// evidence.
///
/// The decisions are *stated*, not applied. This is official's dictionary, whose
/// `tcdInstanceTypes` carry the symbol the rule decided at each position it
/// decides and the goal's own arguments everywhere else, and the framework unifies
/// those against the goal's through the shared unifier. That single step binds an
/// argument that was open, rejects one that was already fixed to something else,
/// and never assigns a rigid variable — which is what official's `unifyTypes` over
/// a rule's decided arguments does to a skolem.
fn decided(arguments: &[InferType], decisions: Vec<Decision>) -> PrimitiveOutcome {
    let mut decided = arguments.to_vec();
    for Decision { position, symbol } in decisions {
        let Some(slot) = decided.get_mut(position) else {
            return PrimitiveOutcome::Undecided;
        };
        *slot = InferType::TypeLevelString(symbol);
    }
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary { arguments: decided },
        deferred: Vec::new(),
    }
}

/// The type-level string `argument` is, or `None` when it is not one.
///
/// An unsolved inference variable is `None` — unknown, not an empty string — and
/// so is anything that is not a literal, which is what makes an argument read
/// identically here and in the kind check that allowed it.
fn known_symbol(argument: &InferType) -> Option<&str> {
    match argument {
        InferType::TypeLevelString(value) => Some(value.as_str()),
        _ => None,
    }
}
