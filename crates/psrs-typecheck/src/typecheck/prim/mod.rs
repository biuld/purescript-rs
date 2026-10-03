//! The primitive rule table: dispatch from a class identity to the rule that
//! decides it, and the contract a rule's result must satisfy.
//!
//! Dispatch is a table lookup keyed by [`hir::TypeId`], consulted once per
//! wanted constraint in a fixed position: a `Proof` member's rule before the
//! lexical givens, and every other member's rule after the givens and
//! superclass paths and before instance search. The position is not chosen per
//! rule: it follows from the member's [`EvidenceClass`], because a
//! compile-time proof is a checked boundary rather than a dictionary and so
//! nothing in scope discharges it, while a relation's dictionary and a report's
//! diagnostic are what an ordinary given or instance supplies.
//!
//! A rule receives ordinary [`InferType`] arguments through [`PrimitiveArgs`]
//! and returns one [`PrimitiveOutcome`]. The framework does not take the
//! outcome on trust: accepting it is a transaction, so a rule that declines
//! leaves no substitution, level, kind, or diagnostic behind, while a rule whose
//! decision contradicts the obligation keeps the diagnostic the shared unifier
//! produced for it. The framework — not the rule — decides both, and it decides
//! the second by unifying the rule's decided arguments against the goal's, which
//! is the step official solving takes on every dictionary it produces. See
//! `dispatch` for the acceptance path, `verify` for that step, and
//! `requeue` for the bound on a deferral.

use crate::typecheck::*;

mod coercible;
mod compare;
mod dispatch;
mod int;
mod requeue;
mod row;
mod symbol;
mod verify;

#[cfg(test)]
mod tests;

/// What a member's evidence is. The classification is the member's result
/// contract: it fixes where the rule is consulted and what a solved obligation
/// lowers to, and no downstream stage re-derives a member's meaning from its
/// name.
// Only `CompileTimeProof` has a rule today, so the other two classifications
// are recorded by the contract rather than constructed. They are part of the
// result contract a rule for a `Relation` or a report declares, and this
// expectation is what asks the next rule to remove it.
// The test build constructs every variant, so the expectation is only for the
// build that does not.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "only the one Proof member has a rule; the other classifications belong to the rules that do not exist yet"
    )
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::typecheck) enum EvidenceClass {
    /// No runtime value: the evidence is a checked boundary that Core lowers to
    /// a representation conversion. A given dictionary cannot supply it, which
    /// is why the rule for such a member is consulted before the givens.
    CompileTimeProof,
    /// A value that erases, or that a dictionary projection reads.
    RuntimeDictionary,
    /// No evidence: a diagnostic is the result.
    ReportOnly,
}

impl EvidenceClass {
    /// Whether this class's evidence is a dictionary a given or an instance
    /// could supply instead of the rule.
    ///
    /// Only a compile-time proof answers `false`, and it does so because its
    /// evidence is a boundary rather than a dictionary: THIR rejects a
    /// coercion whose evidence is not an explicit proof boundary, so a given
    /// for a `Proof` member would produce evidence the program cannot verify.
    fn accepts_a_given(self) -> bool {
        !matches!(self, EvidenceClass::CompileTimeProof)
    }
}

/// One primitive rule, as the table records it.
pub(in crate::typecheck) struct PrimitiveRule {
    /// The class identity the rule decides, and the key of its table entry. A
    /// qualification, an alias, or a re-export reaches the same identity, so
    /// they reach the same rule.
    pub(in crate::typecheck) class_id: hir::TypeId,
    /// What the rule's evidence is.
    pub(in crate::typecheck) evidence: EvidenceClass,
    /// The number of arguments the member's declaration declares. A wanted
    /// constraint that does not carry exactly this many arguments is not an
    /// obligation this rule applies to.
    pub(in crate::typecheck) arity: usize,
    solve: fn(&mut Checker, &PrimitiveArgs) -> PrimitiveOutcome,
}

/// The rules that exist. Every member with a rule appears here and nowhere
/// else, so adding a relation is a table entry and a rule function rather than
/// a branch at the dispatch site.
///
/// A member with no entry reaches instance search, and a member whose rule
/// declines continues into instance search: a relation never depends on an
/// instance being visible, and declining is not a failure.
///
#[cfg(not(test))]
fn rules() -> impl Iterator<Item = &'static PrimitiveRule> {
    core::iter::once(&coercible::RULE)
        .chain(symbol::RULES.iter())
        .chain(compare::RULES.iter())
        .chain(int::RULES.iter())
        .chain(row::RULES.iter())
}

/// The test build adds the framework cases' synthetic rules. They are keyed by
/// identities the registry does not declare, so they exercise the dispatch
/// contract rather than any `Prim` relation.
#[cfg(test)]
fn rules() -> impl Iterator<Item = &'static PrimitiveRule> {
    core::iter::once(&coercible::RULE)
        .chain(symbol::RULES.iter())
        .chain(compare::RULES.iter())
        .chain(int::RULES.iter())
        .chain(row::RULES.iter())
        .chain(tests::SYNTHETIC.iter())
}

/// The rule for `class_id`, if that member has one.
pub(in crate::typecheck) fn primitive_rule(
    class_id: hir::TypeId,
) -> Option<&'static PrimitiveRule> {
    rules().find(|rule| rule.class_id == class_id)
}

/// The evidence a rule produces.
///
/// The shape is the member's strategy: a `Proof` member's evidence is the
/// boundary it checked, a `Relation` member's is an ordinary dictionary that
/// erases and records the arguments the rule decided, and a report has none.
// `Proof` is the only evidence a rule produces today. The other two are the
// contract for the members whose rules do not exist yet, and the tests exercise
// all three through the dispatch.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "a report's absence belongs to the three report members whose rules do not exist yet; a relation's dictionary is already produced by the symbol rules"
    )
)]
#[derive(Clone, Debug)]
pub(in crate::typecheck) enum PrimitiveEvidence {
    /// A checked boundary with no runtime value, recording the types it
    /// connects.
    Proof {
        source: InferType,
        target: InferType,
    },
    /// An ordinary dictionary for a relation, recording the arguments the rule
    /// decided.
    ///
    /// The arguments are the dictionary's own, position by position: the type the
    /// rule decided where it decided one, and the goal's own argument where it did
    /// not. This is official's `tcdInstanceTypes`, and the framework unifies each
    /// of them against the goal's argument at the same position — see `verify` —
    /// so a decision that contradicts the obligation is *found* there rather than
    /// asserted here, and a rule cannot report its own contradiction.
    ///
    /// Because the framework has just unified them against the goal, they are also
    /// the arguments the constraint now has, which is what the evidence records and
    /// what a downstream stage reads.
    Dictionary { arguments: Vec<InferType> },
    /// No evidence: the member reports, and the diagnostic is its result.
    Report,
}

impl PrimitiveEvidence {
    /// The solution a constraint keeps for this evidence, or `None` when the
    /// evidence is deliberately absent.
    pub(in crate::typecheck) fn into_solution(self) -> Option<WantedSolution> {
        match self {
            PrimitiveEvidence::Proof { source, target } => {
                Some(WantedSolution::Coercible { source, target })
            }
            PrimitiveEvidence::Dictionary { arguments } => {
                Some(WantedSolution::Primitive { arguments })
            }
            PrimitiveEvidence::Report => None,
        }
    }
}

/// One rule's answer.
///
/// `Undecided` and `Failed` are separate answers on purpose. Official solving
/// returns `Maybe [TypeClassDict]` and never inspects the goal, so it cannot tell
/// "no rule applies" from "the rule applies and the obligation cannot hold" — it
/// reports the second by failing the unification of its own decided arguments
/// against the goal's. Splitting them here is what lets a `Prim` obligation that
/// is still unknown reach instance search instead of being reported as impossible,
/// and the second case is reached the way official reaches it: the framework runs
/// that unification itself, so no rule answers it and no rule can get it wrong.
// `Solved`, `Undecided`, and `Failed` are the three answers the rules that exist
// return. `Deferred` belongs to the relations and reports whose rules do not
// exist yet; the framework handles it and the tests reach it, so a rule can
// return it without the framework changing.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "no rule defers yet; a deferral belongs to the relations and reports not yet implemented"
    )
)]
pub(in crate::typecheck) enum PrimitiveOutcome {
    /// The rule decides the relation and the evidence is explicit. Any
    /// obligation in `deferred` still has to be discharged.
    ///
    /// A relation's evidence carries what the rule decided, and the framework
    /// unifies it against the goal before believing it: a `Solved` whose decision
    /// contradicts an argument the obligation already fixed is reported, not
    /// searched.
    Solved {
        evidence: PrimitiveEvidence,
        deferred: Vec<WantedConstraint>,
    },
    /// The rule makes progress on the known part without the full answer, which
    /// is how it defers. `evidence` is what it could already decide, and
    /// `deferred` is what it could not.
    Deferred {
        evidence: Option<PrimitiveEvidence>,
        deferred: Vec<WantedConstraint>,
    },
    /// The rule does not apply. The obligation continues into the ordinary
    /// paths.
    Undecided,
    /// The rule applies and the obligation cannot hold, for a reason no unification
    /// finds: the arguments do not permit *any* answer, as `Cons "ab" "c" s`
    /// does not. `code` is an existing diagnostic kind, so the reported
    /// `errorCode` is one `purs` itself raises for an obligation; the framework
    /// refuses a kind that maps to no official code rather than inventing one.
    ///
    /// A contradiction between types the rule *did* decide is not this answer: it
    /// is what the framework's own unification finds, so no rule needs to return
    /// `Failed` for it.
    Failed {
        code: TypeCheckErrorKind,
        detail: String,
    },
}

/// The arguments of one primitive obligation, read the way an instance head's
/// arguments arrive: zonked through the shared substitution where a type is
/// known, carrying unsolved inference variables with their levels and kinds
/// where it is not.
///
/// A rule reads its arguments only through this type, so there is one answer to
/// what an argument is and no rule can recognise a shape differently from
/// another.
pub(in crate::typecheck) struct PrimitiveArgs<'a> {
    constraint: &'a WantedConstraint,
}

impl<'a> PrimitiveArgs<'a> {
    /// The range of the obligation, which every diagnostic a rule reports and
    /// every piece of evidence it produces carries.
    pub(in crate::typecheck) fn span(&self) -> TextRange {
        self.constraint.span
    }

    /// The arguments in declaration order, each read through the shared
    /// substitution.
    pub(in crate::typecheck) fn resolved(&self, checker: &Checker) -> Vec<InferType> {
        self.constraint
            .arguments
            .iter()
            .map(|argument| checker.resolve_type(argument.clone()))
            .collect()
    }

    /// The unsolved, flexible inference variables any argument still mentions.
    /// A rigid variable is not one: a signature's variable is as determined as
    /// a literal, and a rule may decide on it.
    pub(in crate::typecheck) fn unknowns(&self, checker: &Checker) -> HashSet<u32> {
        let mut unknowns = HashSet::new();
        for argument in &self.constraint.arguments {
            let mut variables = HashSet::new();
            classes::collect_infer_variables(
                &checker.resolve_type(argument.clone()),
                &mut variables,
            );
            unknowns.extend(
                variables
                    .into_iter()
                    .filter(|variable| !checker.state.rigid.contains(variable)),
            );
        }
        unknowns
    }

    /// Whether every argument is determined. This is the framework's own
    /// reading, not the rule's: it is what decides whether a `Failed` outcome
    /// may be honoured.
    pub(in crate::typecheck) fn all_determined(&self, checker: &Checker) -> bool {
        self.unknowns(checker).is_empty()
    }
}

/// What consulting the rule table did for one wanted constraint.
pub(in crate::typecheck) enum PrimitiveDispatch {
    /// No rule applies, or its outcome was not accepted. The ordinary paths
    /// continue.
    None,
    /// The rule discharged the obligation with this evidence.
    Solved(WantedSolution),
    /// The obligation cannot hold and is reported: either the rule said so, or
    /// the framework found its decision contradicting the goal. Either way the
    /// diagnostic stands and the solver is not restored.
    Reported,
    /// The rule deferred. The obligation keeps whatever evidence the rule
    /// produced, and its residual obligations have been re-queued.
    Deferred { evidence: Option<WantedSolution> },
}

/// Whether `class_id`'s rule is consulted before the lexical givens.
///
/// This is the one place the dispatch order is decided, and it is decided by the
/// member's evidence class rather than by its identity, so a rule cannot be
/// consulted in one position and discharged in another.
pub(in crate::typecheck) fn primitive_rule_precedes_givens(class_id: hir::TypeId) -> bool {
    primitive_rule(class_id).is_some_and(|rule| !rule.evidence.accepts_a_given())
}
