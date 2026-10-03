//! Solving a wanted constraint: the entry that partitions solved from retained,
//! and the policy deciding what an undischarged obligation becomes.

use super::super::super::*;
use super::super::fundeps::collect_infer_variables;
use crate::typecheck::vocabulary::*;

/// The maximum instance-context search depth. Recursive instances increase the
/// structure of the wanted types, so a finite bound terminates every search
/// and reports a bounded failure rather than looping. A primitive rule's
/// deferral re-entry shares this bound, because re-entering wanted solving is
/// the same search.
const MAX_SOLVE_DEPTH: usize = 64;

/// How deep one search over wanted constraints is.
///
/// This is threaded rather than carried in solver state, so a retained
/// constraint keeps no search history and one declaration's constraints do not
/// see another's. A primitive deferral re-entry is one level deeper than the
/// obligation that deferred it, which is what bounds a rule's chain.
#[derive(Clone, Copy)]
pub(in crate::typecheck) struct SolveDepth {
    depth: usize,
}

impl SolveDepth {
    /// The search one wanted constraint starts.
    pub(in crate::typecheck) fn new() -> Self {
        Self { depth: 0 }
    }

    /// The search one level deeper, for an instance context or a re-entered
    /// obligation.
    pub(in crate::typecheck) fn deeper(self) -> Self {
        Self {
            depth: self.depth + 1,
        }
    }

    /// Whether this search is still within the bound.
    pub(in crate::typecheck) fn within_bound(self) -> bool {
        self.depth <= MAX_SOLVE_DEPTH
    }
}

/// What becomes of a wanted constraint that no given, a superclass path, an
/// instance, or a primitive relation discharges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::typecheck) enum UnsolvedPolicy {
    /// The obligation must be discharged, so an unsolvable constraint is a
    /// `NoInstance` diagnostic. This is the rule for a declaration that carries
    /// a signature, whose own dictionary parameters are the only evidence it has,
    /// and for an instance declaration's members.
    RequireSolved,
    /// A constraint that still mentions a flexible unknown is retained: it
    /// becomes one of the declaration's scheme constraints and one dictionary
    /// parameter, which is what official PureScript's `Entailment.unique`
    /// allows when `solverShouldGeneralize` holds and some argument can still be
    /// generalized. A constraint whose arguments are all decided has nothing to
    /// quantify, so it is still `NoInstance`: generalizing it would hide a
    /// missing instance rather than defer it.
    Retain,
}

impl Checker {
    /// Solves every unsolved wanted constraint against the current givens and
    /// the declared instances. Functional dependencies improve unknown types
    /// before the search (see `fundeps`).
    ///
    /// `wanted_start` is the index at which this declaration's constraints
    /// begin. Entries before it were decided by an earlier declaration, which
    /// has already generalized or reported them, so they are kept as they are:
    /// solving one again here could bind a variable that declaration has since
    /// quantified, and a second diagnostic for it would say nothing new.
    ///
    /// The returned indices are the constraints `unsolved` retained, in the order
    /// they arose; each is the one obligation the declaration's scheme gains.
    ///
    /// When `result` is supplied, the new constraints are also checked for
    /// ambiguity: every remaining type variable must be determined by the
    /// result type and the class functional dependencies.
    pub(in crate::typecheck) fn solve_wanted_constraints(
        &mut self,
        result: Option<&InferType>,
        wanted_start: usize,
        unsolved: UnsolvedPolicy,
    ) -> Vec<usize> {
        let mut wanted = std::mem::take(&mut self.state.wanted);
        let start = wanted_start.min(wanted.len());
        for constraint in &mut wanted[start..] {
            constraint.arguments = constraint
                .arguments
                .iter()
                .map(|argument| self.resolve_type(argument.clone()))
                .collect::<Vec<_>>();
        }
        self.improve_wanted(&mut wanted[start..]);
        let mut solved = Vec::with_capacity(wanted.len());
        let mut retained = Vec::new();
        for (index, mut constraint) in wanted.into_iter().enumerate() {
            if index < start {
                solved.push(constraint);
                continue;
            }
            if constraint.solution.is_none() {
                constraint.arguments = constraint
                    .arguments
                    .iter()
                    .map(|argument| self.resolve_type(argument.clone()))
                    .collect::<Vec<_>>();
                let errors_before = self.state.errors.len();
                let givens = constraint.givens.clone();
                let found = self.with_given_chain(givens, |checker| {
                    checker.solve_constraint(&constraint, SolveDepth::new())
                });
                constraint.solution = found;
                let reported_resolution_error =
                    self.state.errors[errors_before..].iter().any(|error| {
                        matches!(
                            error.kind,
                            TypeCheckErrorKind::OverlappingInstances
                                | TypeCheckErrorKind::NoInstance
                        )
                    });
                if constraint.solution.is_none() && !reported_resolution_error {
                    if unsolved == UnsolvedPolicy::Retain
                        && self.can_generalize_constraint(&constraint)
                    {
                        retained.push(index);
                    } else {
                        let rendered =
                            self.display_constraint(constraint.class_id, &constraint.arguments);
                        self.state.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::NoInstance,
                            constraint.span,
                            format!("no instance for constraint {rendered}"),
                        ));
                    }
                }
            }
            solved.push(constraint);
        }
        // A primitive rule's deferral is re-queued during solving, so it appended
        // to the wanted list while the list above was being walked. Appending
        // after it keeps the index every retained constraint's evidence refers to
        // unchanged, and keeps `wanted_start` pointing at this declaration's own
        // constraints.
        let requeued = std::mem::take(&mut self.state.wanted);
        self.state.wanted = solved;
        self.state.wanted.extend(requeued);
        if let Some(result) = result {
            self.check_ambiguity(result, start);
        }
        retained
    }

    /// A constraint that still mentions something generalization
    /// could quantify. A nullary class constraint is generalized on its own, and
    /// any argument that is still a flexible inference variable makes the
    /// constraint a pending one; an argument that is decided — a concrete type or
    /// a rigid binder — has nothing left to defer.
    ///
    /// This is official PureScript's `canBeGeneralized`, read the same way: a
    /// `C Int` obligation is a missing instance, and `C ?a` is a constraint the
    /// declaration's type can still quantify.
    fn can_generalize_constraint(&self, constraint: &WantedConstraint) -> bool {
        if constraint.arguments.is_empty() {
            return true;
        }
        constraint.arguments.iter().any(|argument| {
            let mut variables = HashSet::new();
            collect_infer_variables(&self.resolve_type(argument.clone()), &mut variables);
            variables
                .iter()
                .any(|variable| !self.state.rigid.contains(variable))
        })
    }
}
