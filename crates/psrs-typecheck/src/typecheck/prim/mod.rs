//! The primitive rule table: dispatch from a class identity to the rule that
//! decides it, and the contract a rule's result must satisfy.
//!
//! Dispatch is a table lookup keyed by [`hir::TypeId`]. Proof boundaries and
//! type-level relation rules precede lexical givens; report rules run after
//! givens so warnings and failures can propagate to an enclosing boundary. The
//! position follows from [`EvidenceClass`]: a proof is not a dictionary, a
//! relation decides its type-level facts, and reports can prefer a dictionary in
//! scope.
//!
//! A rule receives ordinary [`InferType`] arguments through [`PrimitiveArgs`]
//! and returns one [`PrimitiveOutcome`]. The framework verifies any produced
//! dictionary through the shared unifier: a rule that declines leaves no
//! substitution, level, kind, or diagnostic behind, while a contradictory
//! decision keeps the unifier's diagnostic. Applicability belongs to the rule;
//! the framework does not infer it by counting unknowns. See
//! `dispatch` for the acceptance path, `verify` for that step, and
//! `requeue` for the bound on a deferral.

use crate::typecheck::*;

mod coercible;
mod compare;
mod dispatch;
mod int;
mod reports;
pub(in crate::typecheck) mod requeue;
mod row;
mod symbol;
mod verify;

#[cfg(test)]
mod tests;

/// What a member's evidence is. The classification is the member's result
/// contract: it fixes where the rule is consulted and what a solved obligation
/// lowers to, and no downstream stage re-derives a member's meaning from its
/// name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::typecheck) enum EvidenceClass {
    /// No runtime value: the evidence is a checked boundary that Core lowers to
    /// a representation conversion. A given dictionary cannot supply it, which
    /// is why the rule for such a member is consulted before the givens.
    CompileTimeProof,
    /// A value that erases, or that a dictionary projection reads.
    RuntimeDictionary,
    /// A dictionary that also emits a report. `Warn` prefers a dictionary in
    /// scope so its warning can be propagated outward before this rule runs.
    ReportingDictionary,
    /// No evidence: a diagnostic is the result.
    ReportOnly,
}

impl EvidenceClass {
    /// Whether the primitive rule is tried before a lexical given.
    ///
    /// Relations must decide their type-level facts before a caller's
    /// dictionary can mask them. A report runs after givens so `Warn`, `Fail`,
    /// and `Partial` can propagate through an enclosing constraint. A proof
    /// runs first because a dictionary is not the proof boundary Core expects.
    fn precedes_givens(self) -> bool {
        matches!(
            self,
            EvidenceClass::CompileTimeProof | EvidenceClass::RuntimeDictionary
        )
    }

    /// Whether a direct lexical given can supply this evidence.
    ///
    /// `Coercible` is a checked proof boundary, so a given dictionary cannot
    /// discharge it. Its rule reads and composes proof givens itself.
    fn accepts_a_given(self) -> bool {
        matches!(
            self,
            EvidenceClass::RuntimeDictionary
                | EvidenceClass::ReportingDictionary
                | EvidenceClass::ReportOnly
        )
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
        .chain(row::deferred::RULES.iter())
        .chain(reports::RULES.iter())
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
        .chain(row::deferred::RULES.iter())
        .chain(reports::RULES.iter())
        .chain(tests::SYNTHETIC.iter())
}

/// The rule for `class_id`, if that member has one.
pub(in crate::typecheck) fn primitive_rule(
    class_id: hir::TypeId,
) -> Option<&'static PrimitiveRule> {
    rules().find(|rule| rule.class_id == class_id)
}

/// Whether an unresolved primitive class reports a diagnostic rather than
/// producing a user-supplied dictionary. The report table owns this fact.
pub(in crate::typecheck) fn is_report_only(class_id: hir::TypeId) -> bool {
    reports::is_report_only(class_id)
}

/// The evidence a rule produces.
///
/// The shape is the member's strategy: a `Proof` member's evidence is the
/// boundary it checked, a `Relation` member's is an ordinary dictionary that
/// erases and records the arguments the rule decided, and a report has none.
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
/// `Undecided` and `Failed` are separate answers: applicability belongs to the
/// rule and may follow from a decisive known part even while another argument is
/// flexible. The framework does not infer silence or failure by counting
/// unknowns. It verifies produced dictionary arguments through the shared
/// unifier, and `Failed` reports an impossibility established by the rule.
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
    /// The rule decides a known portion but leaves a residual obligation.
    /// `evidence` records the decision already made; `deferred` carries the rest.
    Deferred {
        evidence: Option<PrimitiveEvidence>,
        deferred: Vec<WantedConstraint>,
    },
    /// The rule does not apply. The obligation continues into the ordinary
    /// paths.
    Undecided,
    /// The rule applies and its semantics establish that the obligation cannot
    /// hold, even if another argument remains flexible. `code` is an existing
    /// diagnostic kind, so the reported
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
    /// The source range of the obligation, used for type errors and evidence.
    pub(in crate::typecheck) fn span(&self) -> TextRange {
        self.constraint.span
    }

    /// The owner location for reports such as `Warn`, captured when the wanted
    /// was created. It is separate from the constraint span because `purs`
    /// locates such warnings at the enclosing declaration.
    pub(in crate::typecheck) fn report_span(&self) -> TextRange {
        self.constraint.report_span
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
/// The evidence classification owns this position: type-level relations and
/// proof boundaries precede givens; reports run after givens so they can
/// propagate or emit their diagnostic at the unresolved boundary.
pub(in crate::typecheck) fn primitive_rule_precedes_givens(class_id: hir::TypeId) -> bool {
    primitive_rule(class_id).is_some_and(|rule| rule.evidence.precedes_givens())
}

/// Whether a direct lexical given is forbidden from supplying this member's
/// evidence. `Coercible` is the only such member: its proof rule can consume
/// givens but must produce a verified proof boundary.
pub(in crate::typecheck) fn primitive_rule_skips_given_lookup(class_id: hir::TypeId) -> bool {
    primitive_rule(class_id).is_some_and(|rule| !rule.evidence.accepts_a_given())
}
