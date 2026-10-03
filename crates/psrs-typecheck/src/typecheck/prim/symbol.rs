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
//! head is exactly one scalar. Those are the declines and the definite failures
//! below, and neither is ever answered with a guess.

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
/// carries both the position it fills and that position's name, so a diagnostic
/// says which argument the rule decided.
const APPEND_LEFT: usize = 0;
const APPEND_RIGHT: usize = 1;
const APPEND_APPENDED: usize = 2;
const CONS_HEAD: usize = 0;
const CONS_TAIL: usize = 1;
const CONS_SYMBOL: usize = 2;

/// One argument a rule decided: the position it fills, the name that position
/// has in the member's declaration, and the symbol it is decided to.
struct Decision {
    position: usize,
    argument: &'static str,
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
            argument: "appended",
            symbol: String::from(left) + right,
        }],
        (Some(left), _) => {
            match known_symbol(appended).and_then(|appended| appended.strip_prefix(left)) {
                Some(suffix) => vec![Decision {
                    position: APPEND_RIGHT,
                    argument: "right",
                    symbol: suffix.to_owned(),
                }],
                None => return PrimitiveOutcome::Undecided,
            }
        }
        (_, Some(right)) => {
            match known_symbol(appended).and_then(|appended| appended.strip_suffix(right)) {
                Some(prefix) => vec![Decision {
                    position: APPEND_LEFT,
                    argument: "left",
                    symbol: prefix.to_owned(),
                }],
                None => return PrimitiveOutcome::Undecided,
            }
        }
        _ => return PrimitiveOutcome::Undecided,
    };
    decided(
        checker,
        args,
        hir::TypeId::PRIM_SYMBOL_APPEND,
        &arguments,
        decisions,
    )
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
/// and the rule declines. A head that is empty or longer than one scalar is a
/// definite failure rather than a guess: no reading of the arguments can join it,
/// and an answer that picked one anyway would be inventing a symbol.
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
                        argument: "head",
                        symbol: first.to_string(),
                    },
                    Decision {
                        position: CONS_TAIL,
                        argument: "tail",
                        symbol: scalars.as_str().to_owned(),
                    },
                ],
                None => return PrimitiveOutcome::Undecided,
            }
        }
        None => match (known_symbol(head), known_symbol(tail)) {
            (Some(head), Some(tail)) if head.chars().count() == 1 => vec![Decision {
                position: CONS_SYMBOL,
                argument: "symbol",
                symbol: String::from(head) + tail,
            }],
            (Some(head), Some(_)) => {
                return PrimitiveOutcome::Failed {
                    code: TypeCheckErrorKind::NoInstance,
                    detail: format!(
                        "{} does not hold: the head {head:?} is not exactly one character",
                        checker.display_constraint(hir::TypeId::PRIM_SYMBOL_CONS, &arguments)
                    ),
                };
            }
            _ => return PrimitiveOutcome::Undecided,
        },
    };
    decided(
        checker,
        args,
        hir::TypeId::PRIM_SYMBOL_CONS,
        &arguments,
        decisions,
    )
}

/// Applies `decisions` to the arguments they decide and turns the result into
/// the relation's outcome.
///
/// A decision is expressed through the shared substitution, so the evidence
/// records the arguments the constraint has *after* the decision — the same
/// arguments every later stage sees, rather than a copy of what the rule
/// computed.
fn decided(
    checker: &mut Checker,
    args: &PrimitiveArgs,
    class_id: hir::TypeId,
    arguments: &[InferType],
    decisions: Vec<Decision>,
) -> PrimitiveOutcome {
    let span = args.span();
    for decision in decisions {
        let Decision {
            position,
            argument,
            symbol,
        } = decision;
        match &arguments[position] {
            InferType::TypeLevelString(known) if *known == symbol => {}
            InferType::TypeLevelString(_) => {
                // The rule applies, and the argument it decided is already known
                // to be something else, so the obligation cannot hold. This is
                // the case official solving reports as the failed unification of
                // its own decided argument, and it is the only case in which this
                // relation has a decided value to be wrong about.
                //
                // An earlier decision in the same obligation may already have
                // bound an argument, and that binding is what lets this report be
                // honoured: the framework reads a `Failed` as impossible only
                // once every argument is determined.
                return PrimitiveOutcome::Failed {
                    code: TypeCheckErrorKind::TypeMismatch,
                    detail: format!(
                        "{} does not hold: the {argument} is {symbol:?}",
                        checker.display_constraint(class_id, arguments)
                    ),
                };
            }
            InferType::Variable(variable) if !checker.state.rigid.contains(variable) => {
                if !checker.bind_type_variable(*variable, InferType::TypeLevelString(symbol), span)
                {
                    // The shared unifier refused the binding and reported why, so
                    // there is no decision to record and the rule declines.
                    return PrimitiveOutcome::Undecided;
                }
            }
            // A rigid variable, or a type that is not a type-level string, is a
            // position this rule does not decide: it never assigns a rigid
            // variable, and it does not guess what a symbol of another shape is.
            // A decline after an earlier decision is rolled back with the rest of
            // the rule's speculative work, so a partial decision does not survive.
            _ => return PrimitiveOutcome::Undecided,
        }
    }
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary {
            arguments: arguments
                .iter()
                .map(|argument| checker.resolve_type(argument.clone()))
                .collect(),
        },
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
