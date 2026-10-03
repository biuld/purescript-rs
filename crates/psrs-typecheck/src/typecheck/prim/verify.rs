//! The check every produced dictionary passes before its obligation is
//! discharged: the rule's decided arguments are unified against the goal's.
//!
//! `Language/PureScript/TypeChecker/Entailment.hs:301` is the reference, and it
//! is where the structure this module reproduces comes from:
//!
//! ```haskell
//! lift . lift $ zipWithM_ (\t1 t2 -> do
//!   let inferredType = replaceAllTypeVars (M.toList subst') t1
//!   unifyTypes inferredType t2) (tcdInstanceTypes tcd) tys''
//! ```
//!
//! Official solves by producing, never by reporting: a rule returns
//! `Maybe [TypeClassDict]` and never inspects the goal, and the solver unifies
//! each produced dictionary's own argument types — the positions a rule decided
//! and the positions it copied from the goal — against the goal's arguments, in
//! order, for every dictionary it produces. A failed unification aborts that goal
//! and reports `TypesDoNotUnify`.
//!
//! So the answer to "is this decision consistent with the obligation?" is not
//! something a rule is asked. It is one operation over a rule's declared
//! arguments and the goal, and a rule cannot omit it, get it wrong, or answer it
//! on its own authority. That matters most where no rule can be trusted to
//! notice: a decision over two arguments that are *already known* binds nothing,
//! so a framework reading "did anything become more determined?" as the test for
//! whether a rule had an opinion has no way to tell that opinion apart from
//! silence — and discards a contradiction the obligation actually has.

use super::{PrimitiveArgs, PrimitiveEvidence};
use crate::typecheck::*;

impl Checker {
    /// Whether `evidence`'s decided arguments unify with the goal's, reporting a
    /// disagreement when they do not.
    ///
    /// This is the whole of official's `zipWithM_` step: each decided argument is
    /// unified with the goal's argument at the same position, through
    /// [`Checker::unify`], so a decision is a binding the rest of inference sees,
    /// its kinds are checked by the shared binder, and two rows are compared by
    /// the shared row unifier rather than structurally.
    ///
    /// A proof carries no dictionary arguments and has nothing to unify, so it is
    /// accepted as it is; only a relation's decided arguments are checked.
    ///
    /// The check runs as a *reporting* trial. A failed unification rolls back
    /// whatever it had bound so far and keeps its first diagnostic, so the
    /// diagnostic survives because it was kept rather than because nothing was
    /// restored — and a rule's own bindings are not left behind describing an
    /// obligation that cannot hold.
    ///
    /// A `false` here is the only way this returns false, and it is the
    /// framework's own answer: nothing a rule returns can stand in for it.
    pub(in crate::typecheck) fn decided_arguments_unify(
        &mut self,
        args: &PrimitiveArgs,
        evidence: &PrimitiveEvidence,
    ) -> bool {
        let PrimitiveEvidence::Dictionary { arguments } = evidence else {
            return true;
        };
        let wanted = args.resolved(self);
        let span = args.span();
        let pairs: Vec<(InferType, InferType)> = arguments.iter().cloned().zip(wanted).collect();
        self.speculate_reporting(|checker| {
            for (decided, wanted) in pairs {
                let errors_before = checker.state.errors.len();
                checker.unify(decided, wanted, span);
                if checker.state.errors.len() > errors_before {
                    // Official's `zipWithM_` aborts the goal at the first pair
                    // that fails, so that pair is the one reported and the rest of
                    // the check does not run.
                    checker.state.errors.truncate(errors_before + 1);
                    return None;
                }
            }
            Some(())
        })
        .is_some()
    }
}
