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
//! outcome on trust: accepting it is a transaction, so a rule that declines or
//! that claims a decisive outcome its own arguments contradict leaves no
//! substitution, level, kind, or diagnostic behind. See
//! [`Checker::solve_primitive`] for the acceptance rules and `requeue` for the
//! bound on a deferral.

use crate::typecheck::classes::SolveDepth;
use crate::typecheck::*;

mod coercible;
mod requeue;

use requeue::RequeueChain;
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
}

/// The test build adds the framework cases' synthetic rules. They are keyed by
/// identities the registry does not declare, so they exercise the dispatch
/// contract rather than any `Prim` relation.
#[cfg(test)]
fn rules() -> impl Iterator<Item = &'static PrimitiveRule> {
    core::iter::once(&coercible::RULE).chain(tests::SYNTHETIC.iter())
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
        reason = "only the Coercible rule exists; a relation's dictionary and a report's absence belong to the rules that do not exist yet"
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
    /// decided. The dictionary is empty and erases when the relation is only
    /// about types, but the decision is what the evidence records.
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
/// returns `Maybe [TypeClassDict]`, so it cannot tell "no rule applies" from
/// "the rule applies and the obligation cannot hold", and it reports the
/// second as a missing instance. Splitting them is what lets a `Prim`
/// obligation that is still unknown reach instance search instead of being
/// reported as impossible.
// `Solved` and `Undecided` are the two answers `Coercible` returns. `Deferred`
// and `Failed` belong to the relations and reports whose rules do not exist yet;
// the framework handles both and the tests reach both, so a rule can return them
// without the framework changing.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the only rule today is a Proof, which never defers and never fails; the remaining outcomes belong to the relations and reports not yet implemented"
    )
)]
pub(in crate::typecheck) enum PrimitiveOutcome {
    /// The rule decides the relation and the evidence is explicit. Any
    /// obligation in `deferred` still has to be discharged.
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
    /// The rule applies and the obligation cannot hold. `code` is an existing
    /// diagnostic kind, so the reported `errorCode` is one `purs` itself
    /// raises for an obligation; the framework refuses a kind that maps to no
    /// official code rather than inventing one.
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
    /// The rule reported the obligation under an official code.
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

impl Checker {
    /// Consults the rule table for `constraint`, after improvement has run
    /// again on its arguments, and acts on the outcome.
    ///
    /// The rule's work is speculative: the framework snapshots the solver,
    /// runs the rule, and only keeps what it produced when it accepts the
    /// outcome. A decline, a failed acceptance, or a rollback therefore leaves
    /// no substitution, level, kind, or diagnostic behind, which is the same
    /// contract an instance candidate has.
    pub(in crate::typecheck) fn solve_primitive(
        &mut self,
        constraint: &WantedConstraint,
        depth: SolveDepth,
    ) -> PrimitiveDispatch {
        let Some(rule) = primitive_rule(constraint.class_id) else {
            return PrimitiveDispatch::None;
        };
        // A wanted constraint whose arguments are not the member's own is not
        // this rule's obligation, and the rule declines it rather than guessing
        // at positions that do not exist.
        if constraint.arguments.len() != rule.arity {
            return PrimitiveDispatch::None;
        }
        // Improvement runs again here, because the improvement pass over the
        // wanted list runs before solving starts and cannot reach a constraint
        // that instance solving builds afterwards. A rule therefore usually
        // receives determined arguments whether it was reached from the wanted
        // list or from an instance context.
        let mut improved = constraint.clone();
        self.improve_one(&mut improved);
        let unknowns_before = PrimitiveArgs {
            constraint: &improved,
        }
        .unknowns(self);
        let snapshot = self.state.snapshot();
        let outcome = self.call_rule(
            rule,
            &PrimitiveArgs {
                constraint: &improved,
            },
        );
        // The chain starts at the obligation being decided, so a rule that defers
        // to the very obligation it was asked about is recognised as making no
        // progress rather than retried.
        let mut chain = RequeueChain::seeded(self, &improved);
        let dispatch = self.accept_primitive_outcome(
            constraint,
            &improved,
            &outcome,
            &unknowns_before,
            depth,
            &mut chain,
        );
        if matches!(dispatch, PrimitiveDispatch::None) {
            self.state.restore(snapshot);
        }
        dispatch
    }

    fn call_rule(&mut self, rule: &PrimitiveRule, args: &PrimitiveArgs) -> PrimitiveOutcome {
        (rule.solve)(self, args)
    }

    /// Turns a rule's answer into what constraint solving does next, and refuses
    /// the answers the rule's own arguments contradict.
    ///
    /// The two refusals are the framework's structural form of "an unsolved
    /// inference variable in any argument is never enough for `Solved` or
    /// `Failed`":
    ///
    /// - `Failed` is honoured only when every argument is determined. An
    ///   obligation whose argument is still unknown is not impossible, it is
    ///   undecided, and reporting it is exactly the failure this separation
    ///   exists to prevent.
    /// - `Solved` is honoured only when the rule determined an argument that
    ///   was unknown, or when every argument is now determined. A rule that
    ///   decides on a known part and leaves the rest unknown does so by
    ///   binding the rest through the shared substitution, so "nothing became
    ///   more determined" means the rule decided on nothing at all. Such an
    ///   answer is read as the deferral it should have been.
    ///
    /// A `Failed` code with no official `errorCode` is refused the same way,
    /// so a rule cannot invent a diagnostic the suite has never seen.
    fn accept_primitive_outcome(
        &mut self,
        constraint: &WantedConstraint,
        improved: &WantedConstraint,
        outcome: &PrimitiveOutcome,
        unknowns_before: &HashSet<u32>,
        depth: SolveDepth,
        chain: &mut RequeueChain,
    ) -> PrimitiveDispatch {
        let args = PrimitiveArgs {
            constraint: improved,
        };
        match outcome {
            PrimitiveOutcome::Undecided => PrimitiveDispatch::None,
            PrimitiveOutcome::Solved { evidence, deferred } => {
                let unknowns_after = args.unknowns(self);
                let progressed =
                    unknowns_after.len() < unknowns_before.len() || unknowns_after.is_empty();
                if !progressed {
                    return self.defer_without_evidence(deferred, depth, chain);
                }
                let Some(solution) = evidence.clone().into_solution() else {
                    return self.defer_without_evidence(deferred, depth, chain);
                };
                self.requeue_all(deferred.clone(), depth, chain);
                PrimitiveDispatch::Solved(solution)
            }
            PrimitiveOutcome::Deferred { evidence, deferred } => {
                let solution = evidence.clone().and_then(PrimitiveEvidence::into_solution);
                self.requeue_all(deferred.clone(), depth, chain);
                PrimitiveDispatch::Deferred { evidence: solution }
            }
            PrimitiveOutcome::Failed { code, detail } => {
                if !args.all_determined(self) || code.error_code().is_none() {
                    return PrimitiveDispatch::None;
                }
                self.state
                    .errors
                    .push(TypeCheckError::new(*code, constraint.span, detail.clone()));
                PrimitiveDispatch::Reported
            }
        }
    }

    /// Records the outcome a rule should have returned when it decided nothing:
    /// its residual obligations are re-queued and the obligation itself keeps
    /// no evidence, so instance search still sees it.
    fn defer_without_evidence(
        &mut self,
        deferred: &[WantedConstraint],
        depth: SolveDepth,
        chain: &mut RequeueChain,
    ) -> PrimitiveDispatch {
        self.requeue_all(deferred.to_vec(), depth, chain);
        PrimitiveDispatch::Deferred { evidence: None }
    }
}
