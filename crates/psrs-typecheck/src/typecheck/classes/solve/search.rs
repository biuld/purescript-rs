//! The search over givens, superclass paths, and instances, and the primitive
//! rule's entry into it.

use super::super::super::prim::{PrimitiveDispatch, primitive_rule_precedes_givens};
use super::super::super::unify::substitute;
use super::super::fundeps::collect_infer_variables;
use super::entry::SolveDepth;
use crate::typecheck::*;
impl Checker {
    /// Solves one wanted constraint, consulting givens, the primitive rule
    /// table, and instance search in that order.
    ///
    /// The order is the one the primitive design fixes, and the one place it
    /// varies is where a `Proof` member's rule is consulted:
    /// [`primitive_rule_precedes_givens`] is the predicate, and it is false for
    /// every other member. A `Proof` member's evidence is a checked boundary
    /// rather than a dictionary, so a matching given cannot supply it — THIR
    /// rejects a coercion whose evidence is not an explicit proof boundary — and
    /// the rule has to derive the proof before the givens are consulted, which is
    /// also what official solving does.
    ///
    /// An instance's context is solved recursively before the instance is
    /// selected.
    pub(in crate::typecheck) fn solve_constraint(
        &mut self,
        constraint: &WantedConstraint,
        depth: SolveDepth,
    ) -> Option<WantedSolution> {
        if !depth.within_bound() {
            return None;
        }
        let class_id = constraint.class_id;
        let arguments = constraint.arguments.clone();
        let solves_before_givens = primitive_rule_precedes_givens(class_id);
        // The rule of a member that precedes the givens is also what reads them: a
        // `Coercible` rule composes the assumed proofs itself through the shared
        // given solver. Repeating the direct given lookup here would return the
        // dictionary parameter, which is not the evidence a `Proof` member's
        // boundary lowers to. A superclass path is a different mechanism, so it is
        // still consulted, after the rule has declined.
        if !solves_before_givens && let Some(solution) = self.given_solution(constraint, depth) {
            return Some(solution);
        }
        match self.solve_primitive(constraint, depth) {
            PrimitiveDispatch::None => {}
            PrimitiveDispatch::Solved(solution) => return Some(solution),
            // The rule reported the obligation itself, under an official code. A
            // second diagnostic here would be the same rejection twice.
            PrimitiveDispatch::Reported => return None,
            // A deferral discharges the obligation only when the rule decided part
            // of it and the re-queued obligations carry the rest. With no evidence
            // there is nothing to keep, so the obligation continues into the
            // ordinary paths and is reported there if nothing supplies it.
            PrimitiveDispatch::Deferred {
                evidence: Some(solution),
            } => return Some(solution),
            PrimitiveDispatch::Deferred { evidence: None } => {}
        }
        if solves_before_givens && let Some(solution) = self.superclass_solution(constraint, depth)
        {
            return Some(solution);
        }
        let mut selected = self.select_instance_groups(class_id, &arguments);

        if selected.len() > 1 {
            let rendered = self.display_constraint(class_id, &arguments);
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::OverlappingInstances,
                constraint.span,
                format!("overlapping instances for constraint {rendered}"),
            ));
            return None;
        }
        let (instance, mapping) = selected.pop()?;
        let errors_before = self.state.errors.len();
        let mapping = self.instantiate_selected_instance(&instance, &mapping, constraint);
        if self.state.errors.len() != errors_before {
            return None;
        }
        // Selection commits to the first matching head before solving its
        // context. A context failure cannot fall through to another branch.
        if instance.context.is_empty() {
            return Some(WantedSolution::Global(instance.symbol));
        }
        self.solve_instance(&instance, &mapping, constraint, depth)
    }

    /// The evidence a lexical given supplies for `constraint`, or the evidence a
    /// superclass projection from one does.
    fn given_solution(
        &mut self,
        constraint: &WantedConstraint,
        depth: SolveDepth,
    ) -> Option<WantedSolution> {
        for (given, solution) in self.scope.givens.clone() {
            if given.class_id == constraint.class_id
                && self.constraint_arguments_match_or_unify(
                    &given.arguments,
                    &constraint.arguments,
                    constraint.span,
                )
            {
                return Some(solution);
            }
        }
        self.superclass_solution(constraint, depth)
    }

    /// Derives a wanted superclass constraint from a given subclass dictionary
    /// and its superclass closure.
    fn superclass_solution(
        &mut self,
        wanted: &WantedConstraint,
        depth: SolveDepth,
    ) -> Option<WantedSolution> {
        for (given, solution) in self.scope.givens.clone() {
            if let Some(found) =
                self.superclass_path(given.class_id, &given.arguments, solution, wanted, depth)
            {
                return Some(found);
            }
        }
        None
    }

    fn superclass_path(
        &mut self,
        base_class: hir::TypeId,
        base_arguments: &[InferType],
        base_solution: WantedSolution,
        wanted: &WantedConstraint,
        depth: SolveDepth,
    ) -> Option<WantedSolution> {
        if !depth.within_bound() {
            return None;
        }
        if !self.env.classes.contains_key(&base_class) {
            return None;
        }
        for (field, edge) in self.superclass_constraints(base_class, base_arguments) {
            let parent = self.build_solution_constraint(
                base_class,
                base_arguments.to_vec(),
                wanted.span,
                base_solution.clone(),
            );
            let solution = WantedSolution::Superclass {
                parent: Box::new(parent),
                field,
            };
            if edge.class_id == wanted.class_id
                && self.constraint_arguments_match_or_unify(
                    &edge.arguments,
                    &wanted.arguments,
                    wanted.span,
                )
            {
                return Some(solution);
            }
            if let Some(found) = self.superclass_path(
                edge.class_id,
                &edge.arguments,
                solution,
                wanted,
                depth.deeper(),
            ) {
                return Some(found);
            }
        }
        None
    }

    /// Matches a wanted constraint against a given or projected superclass.
    /// Wanted type variables can be refined to the known argument types, but a
    /// failed candidate must leave no substitution, level, kind, or evidence
    /// behind. Its diagnostic is discarded, because a candidate that does not
    /// match is not itself an error.
    fn constraint_arguments_match_or_unify(
        &mut self,
        expected: &[InferType],
        actual: &[InferType],
        span: TextRange,
    ) -> bool {
        if expected.len() != actual.len() {
            return false;
        }
        self.speculate(|checker| {
            let errors_before = checker.state.errors.len();
            for (expected, actual) in expected.iter().zip(actual) {
                checker.unify(actual.clone(), expected.clone(), span);
            }
            (checker.state.errors.len() == errors_before).then_some(())
        })
        .is_some()
    }

    /// Solves one instance's context and, on success, returns the instance
    /// selection with the context dictionaries and the constructor's full
    /// arrow type.
    fn solve_instance(
        &mut self,
        instance: &InstanceInfo,
        mapping: &HashMap<u32, InferType>,
        constraint: &WantedConstraint,
        depth: SolveDepth,
    ) -> Option<WantedSolution> {
        let mut context = Vec::with_capacity(instance.context.len());
        for child in &instance.context {
            let arguments = child
                .arguments
                .iter()
                .map(|argument| self.resolve_type(substitute(argument, mapping)))
                .collect::<Vec<_>>();
            let mut wanted = self.build_constraint(child.class_id, arguments, child.span);
            let errors_before = self.state.errors.len();
            let Some(solution) = self.solve_constraint(&wanted, depth.deeper()) else {
                let has_nested_diagnostic =
                    self.state.errors[errors_before..].iter().any(|error| {
                        matches!(
                            error.kind,
                            TypeCheckErrorKind::NoInstance
                                | TypeCheckErrorKind::OverlappingInstances
                        )
                    });
                if !has_nested_diagnostic {
                    let rendered = self.display_constraint(child.class_id, &wanted.arguments);
                    self.state.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::NoInstance,
                        wanted.span,
                        format!("no instance for constraint {rendered}"),
                    ));
                }
                return None;
            };
            wanted.solution = Some(solution);
            context.push(wanted);
        }
        let result = self.dictionary_type(&ClassConstraint {
            class_id: constraint.class_id,
            arguments: constraint.arguments.clone(),
            span: constraint.span,
        });
        let mut constructor_type = result;
        for child in context.iter().rev() {
            constructor_type = arrow(child.dictionary_type.clone(), constructor_type);
        }
        Some(WantedSolution::Instance {
            constructor: instance.symbol,
            constructor_type,
            context,
        })
    }

    /// Freshens a selected instance for this particular use and unifies every
    /// head position with the wanted constraint. Functional dependencies may
    /// let matching choose a branch before all positions are known, so those
    /// positions must be connected to fresh instance variables before solving
    /// the instance context.
    fn instantiate_selected_instance(
        &mut self,
        instance: &InstanceInfo,
        mapping: &HashMap<u32, InferType>,
        constraint: &WantedConstraint,
    ) -> HashMap<u32, InferType> {
        let mut mapping = mapping.clone();
        let mut variables = HashSet::new();
        for argument in &instance.head_arguments {
            collect_infer_variables(argument, &mut variables);
        }
        for child in &instance.context {
            for argument in &child.arguments {
                collect_infer_variables(argument, &mut variables);
            }
        }
        for variable in variables {
            if let std::collections::hash_map::Entry::Vacant(entry) = mapping.entry(variable) {
                entry.insert(self.fresh());
            }
        }

        for (head, wanted) in instance.head_arguments.iter().zip(&constraint.arguments) {
            self.unify(substitute(head, &mapping), wanted.clone(), constraint.span);
        }
        mapping
            .into_iter()
            .map(|(variable, ty)| (variable, self.resolve_type(ty)))
            .collect()
    }
}
