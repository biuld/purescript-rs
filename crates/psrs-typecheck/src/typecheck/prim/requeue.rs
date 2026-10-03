//! Deferral is re-queued, not forgotten.
//!
//! A rule that cannot finish an obligation returns the obligations it could not
//! decide. Each of those re-enters wanted solving with its own origin retained,
//! after improvement has run again on its arguments, and so gets another chance
//! at an argument that is more determined than it was. The re-entered
//! obligation is solved there and then, inside this declaration's inference:
//! re-queueing never depends on a residual constraint surviving generalization,
//! so a rule that defers is useful whether or not residual retention has
//! landed.
//!
//! Two bounds keep that from being an unbounded rewrite. Every re-entry is one
//! search level deeper, so it shares the depth bound the ordinary solver
//! already applies. And a deferral that returns an obligation its chain has
//! already tried made no progress toward a determined argument, so it is
//! reported under the official code for an obligation nothing can decide rather
//! than retried.

use crate::typecheck::classes::{SolveDepth, UnsolvedPolicy};
use crate::typecheck::*;

/// The maximum number of obligations one primitive obligation's deferral chain
/// may re-enter wanted solving on.
///
/// This bounds the *width* of one rule's chain, which the search depth does not
/// bound: a rule that defers ten obligations each of which defers ten more is a
/// finite tree of unbounded size, and this is what counts the work of walking
/// it.
const MAX_REQUEUE_STEPS: usize = 32;

/// One primitive obligation's deferral chain: the obligations it has already
/// re-entered wanted solving on.
///
/// An obligation is identified by its class and its arguments, because that is
/// what improvement makes more determined. `Row.Lacks "a" ("b" | r)` and
/// `Row.Lacks "a" r` are different obligations and the first legitimately
/// defers to the second, so that is progress; a chain that produces the second
/// twice has made none, because the second is the one just tried and retrying
/// it cannot finish.
#[derive(Default)]
pub(in crate::typecheck) struct RequeueChain {
    active: Vec<(hir::TypeId, Vec<InferType>)>,
    steps: usize,
}

impl RequeueChain {
    /// The chain a rule's deferrals are measured against, starting at the
    /// obligation the rule was asked about.
    pub(in crate::typecheck) fn seeded(checker: &Checker, constraint: &WantedConstraint) -> Self {
        let mut chain = Self::default();
        let arguments = constraint
            .arguments
            .iter()
            .map(|argument| checker.resolve_type(argument.clone()))
            .collect();
        chain.active.push((constraint.class_id, arguments));
        chain
    }

    /// Enters one requeued obligation unless this path repeats it or the whole
    /// deferral tree has used its bounded re-entry budget.
    fn enter(&mut self, checker: &Checker, constraint: &WantedConstraint) -> bool {
        if self.steps >= MAX_REQUEUE_STEPS {
            return false;
        }
        let arguments = constraint
            .arguments
            .iter()
            .map(|argument| checker.resolve_type(argument.clone()))
            .collect::<Vec<_>>();
        if self.active.iter().any(|(class_id, active_arguments)| {
            *class_id == constraint.class_id
                && active_arguments.len() == arguments.len()
                && active_arguments
                    .iter()
                    .zip(&arguments)
                    .all(|(left, right)| checker.infer_types_equal(left, right))
        }) {
            return false;
        }
        self.steps += 1;
        self.active.push((constraint.class_id, arguments));
        true
    }

    fn leave(&mut self) {
        self.active.pop();
    }
}

impl Checker {
    /// Re-enters wanted solving for every obligation a rule deferred, on one
    /// shared chain so the whole deferral tree is bounded together.
    pub(in crate::typecheck) fn requeue_all(
        &mut self,
        deferred: Vec<WantedConstraint>,
        depth: SolveDepth,
        policy: UnsolvedPolicy,
        chain: &mut RequeueChain,
    ) {
        for constraint in deferred {
            self.requeue(constraint, depth, policy, chain);
        }
    }

    /// Re-enters wanted solving for one deferred obligation, and retains it so
    /// its solution becomes evidence.
    ///
    /// The obligation keeps its own origin, so a diagnostic about it is at its
    /// range rather than at the range of the obligation that deferred it.
    pub(in crate::typecheck) fn requeue(
        &mut self,
        mut constraint: WantedConstraint,
        depth: SolveDepth,
        policy: UnsolvedPolicy,
        chain: &mut RequeueChain,
    ) {
        // Improvement runs again here, before the rule gets another chance, so
        // the re-entry is at least as informed as the deferral was.
        self.improve_one(&mut constraint);
        constraint.arguments = constraint
            .arguments
            .iter()
            .map(|argument| self.resolve_type(argument.clone()))
            .collect();
        if chain.enter(self, &constraint) {
            let errors_before = self.state.errors.len();
            let solution =
                self.solve_constraint_with_chain(&constraint, depth.deeper(), policy, chain);
            chain.leave();
            if let Some(solution) = solution {
                constraint.solution = Some(solution);
                self.state.wanted.push(constraint);
                return;
            }
            // The re-entry reports its own failure when a failure is the answer;
            // a reported obligation is not retained or retried.
            if self.state.errors[errors_before..]
                .iter()
                .any(|error| error.kind.reports_constraint_failure())
            {
                return;
            }
            if policy == UnsolvedPolicy::Retain && self.can_generalize_constraint(&constraint) {
                self.state.wanted.push(constraint);
                return;
            }
        }
        self.report_stalled_deferral(&constraint);
    }

    /// Reports a deferral that returned an obligation its chain had already
    /// tried, or that ran out of the chain's room.
    ///
    /// Nothing about the obligation became more determined, so it is reported
    /// rather than retried. The code is the one official solving raises for a
    /// relation it cannot decide from the arguments it has, and a rule may not
    /// keep an obligation alive by deferring it to itself.
    fn report_stalled_deferral(&mut self, constraint: &WantedConstraint) {
        let rendered = self.display_constraint(constraint.class_id, &constraint.arguments);
        self.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::NoInstance,
            constraint.span,
            format!(
                "no instance for constraint {rendered}: its primitive rule deferred without determining an argument"
            ),
        ));
    }
}
