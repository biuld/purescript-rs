//! Dispatch from a class identity to its rule, and what the framework does with
//! the answer: the transaction a rule's speculative work runs under, and the two
//! refusals that keep an unknown argument from being decisive.
//!
//! `Checker::solve_primitive` is the only entry into the table. It is where
//! official solving's structure lives, and the order is that structure: improve,
//! produce, check, then report. `super::verify` holds the check and `super::requeue`
//! holds the bound on a deferral.

use super::requeue::RequeueChain;
use super::*;
use crate::typecheck::classes::SolveDepth;

impl Checker {
    /// Consults the rule table for `constraint`, after improvement has run
    /// again on its arguments, and acts on the outcome.
    ///
    /// The rule's work is speculative: the framework snapshots the solver,
    /// runs the rule, and keeps what it produced only when it accepts the
    /// outcome. A decline or a failed acceptance therefore leaves no
    /// substitution, level, kind, or diagnostic behind, which is the same
    /// contract an instance candidate has.
    ///
    /// A decision that *contradicts* the obligation is the one thing the snapshot
    /// does not discard. Official solving produces dictionaries and then unifies
    /// each one's arguments against the goal's, and a failure there reports; it
    /// never rolls the failure back into a search that would report something
    /// else. So the framework runs that same check itself, through the shared
    /// unifier, and a contradiction keeps its diagnostic.
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
    /// The acceptance order is what official solving's is: produce, then check,
    /// and report when the check fails.
    ///
    /// 1. A relation's decided arguments are unified against the goal's through
    ///    the shared unifier, which is `Entailment.hs:301`. A disagreement is
    ///    reported and kept, so a rule that decided a value contradicting an
    ///    argument the obligation already fixed is refused here — whether or not
    ///    it bound anything, which is the case neither gate below can see.
    /// 2. The two refusals are the framework's structural form of "an unsolved
    ///    inference variable in any argument is never enough for `Solved` or
    ///    `Failed`":
    ///
    ///    - `Failed` is honoured only when every argument is determined. An
    ///      obligation whose argument is still unknown is not impossible, it is
    ///      undecided, and reporting it is exactly the failure this separation
    ///      exists to prevent.
    ///    - `Solved` is honoured only when the rule determined an argument that
    ///      was unknown, or when every argument is now determined. A rule that
    ///      decides on a known part and leaves the rest unknown does so by
    ///      binding the rest through the shared substitution, so "nothing became
    ///      more determined" means the rule decided on nothing at all. Such an
    ///      answer is read as the deferral it should have been.
    ///
    /// Step 1 comes first precisely because step 2 cannot distinguish "the rule
    /// decided nothing" from "the rule proved the obligation cannot hold": both
    /// bind nothing, and only the unifier knows which of the two happened. A
    /// rule that guesses from a partly-unknown argument is therefore still
    /// downgraded, because the check above found no contradiction and bound
    /// nothing either.
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
                if !self.decided_arguments_unify(&args, evidence) {
                    return PrimitiveDispatch::Reported;
                }
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
                let Some(evidence) = evidence else {
                    self.requeue_all(deferred.clone(), depth, chain);
                    return PrimitiveDispatch::Deferred { evidence: None };
                };
                if !self.decided_arguments_unify(&args, evidence) {
                    return PrimitiveDispatch::Reported;
                }
                let solution = evidence.clone().into_solution();
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
