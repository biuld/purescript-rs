//! Generalization: one sequence over a declaration's type and its retained
//! constraints.
//!
//! The unknowns that survive become the scheme's quantified variables, each with
//! the kind the kind layer recorded for it, and the constraints the declaration
//! did not discharge travel with them. Abstracting a dictionary per retained
//! constraint is the caller's next step; this module decides *what* is
//! generalized, not how the dictionaries are introduced.

use super::*;

impl Checker {
    /// Generalizes a declaration into a scheme.
    ///
    /// `declared` are the variables the declaration's own `forall` binders
    /// introduced. They are quantified whatever level the solver recorded for
    /// them, because they are the polymorphism the source declared rather than a
    /// side effect of the level a binder was allocated at. A binder that no
    /// surviving part of the type or the constraints mentions is not quantified:
    /// a quantifier no occurrence refers to has no meaning.
    ///
    /// `outer_level` is the level the declaration's scope sits at. A variable
    /// at that level or below belongs to the enclosing scope and is left alone,
    /// and a `forall` nested inside the type binds its own binders, so a
    /// structural quantifier is never captured.
    pub(super) fn generalize(
        &mut self,
        declared: &[u32],
        ty: &InferType,
        constraints: &[ClassConstraint],
        outer_level: u32,
    ) -> Scheme {
        let resolved = self.resolve_type(ty.clone());
        let mut variables = Vec::new();
        self.collect_generalizable(&resolved, outer_level, &mut variables);
        for constraint in constraints {
            for argument in &constraint.arguments {
                let resolved = self.resolve_type(argument.clone());
                self.collect_generalizable(&resolved, outer_level, &mut variables);
            }
        }
        for variable in declared {
            if constraint_or_type_mentions(&resolved, constraints, *variable) {
                variables.push(*variable);
            }
        }
        self.scheme(variables, constraints.to_vec(), resolved)
    }

    /// Generalizes an instance dictionary over every variable in its head,
    /// including variables erased from the runtime dictionary shape. Compiler
    /// evidence such as `Coercible (Additive a) a` still mentions those
    /// variables while the dictionary constructor is checked and finalized.
    pub(in crate::typecheck) fn generalize_instance_dictionary(
        &mut self,
        head_variables: &[u32],
        ty: &InferType,
    ) -> Scheme {
        // Instance head variables belong to the dictionary constructor even if
        // the runtime dictionary erases them. Its compile-time evidence still
        // mentions them, so retain them alongside variables in the value type.
        let inferred = self.generalize(&[], ty, &[], TOP_LEVEL);
        let mut variables = inferred.variables;
        variables.extend(head_variables.iter().copied());
        self.scheme(variables, inferred.constraints, inferred.ty)
    }

    /// The scheme a declaration exposes to the uses inside its binding group: the
    /// type its signature states, with that signature's `forall` binders
    /// quantified. A recursive use instantiates it, so a signature's
    /// polymorphism is per use rather than shared by the whole group.
    pub(super) fn declared_scheme(
        &mut self,
        declared: &[u32],
        constraints: Vec<ClassConstraint>,
        ty: InferType,
    ) -> Scheme {
        self.scheme(declared.to_vec(), constraints, ty)
    }

    /// Completes a set of quantified variables into a scheme, recording each
    /// variable's kind through the kind owner.
    fn scheme(
        &mut self,
        mut variables: Vec<u32>,
        constraints: Vec<ClassConstraint>,
        ty: InferType,
    ) -> Scheme {
        variables.sort_unstable();
        variables.dedup();
        // The kind is read through `kind.rs`, which is the one owner of the kind
        // of an inference type. A variable that reached inference without one
        // gets it recorded here, so a quantified variable always carries a kind.
        let variable_kinds = variables
            .iter()
            .map(|variable| (*variable, self.kind_of_variable(*variable)))
            .collect();
        for variable in &variables {
            self.state.generic_variables.insert(*variable);
        }
        Scheme {
            variables,
            variable_kinds,
            constraints,
            ty,
        }
    }

    fn collect_generalizable(&self, ty: &InferType, outer_level: u32, out: &mut Vec<u32>) {
        self.collect_generalizable_scoped(ty, outer_level, out, &mut HashSet::new());
    }

    fn collect_generalizable_scoped(
        &self,
        ty: &InferType,
        outer_level: u32,
        out: &mut Vec<u32>,
        bound: &mut HashSet<u32>,
    ) {
        match ty {
            InferType::Variable(variable) => {
                if !bound.contains(variable)
                    && self
                        .state
                        .levels
                        .get(variable)
                        .copied()
                        .unwrap_or(TOP_LEVEL)
                        > outer_level
                {
                    out.push(*variable);
                }
            }
            InferType::Application(function, argument) => {
                self.collect_generalizable_scoped(function, outer_level, out, bound);
                self.collect_generalizable_scoped(argument, outer_level, out, bound);
            }
            InferType::ForAll { variables, body } => {
                let newly_bound = variables
                    .iter()
                    .filter(|id| !bound.contains(id))
                    .copied()
                    .collect::<Vec<_>>();
                bound.extend(newly_bound.iter().copied());
                self.collect_generalizable_scoped(body, outer_level, out, bound);
                for id in newly_bound {
                    bound.remove(&id);
                }
            }
            InferType::Constrained { constraints, body } => {
                for argument in constraints
                    .iter()
                    .flat_map(|constraint| &constraint.arguments)
                {
                    self.collect_generalizable_scoped(argument, outer_level, out, bound);
                }
                self.collect_generalizable_scoped(body, outer_level, out, bound);
            }
            InferType::RowExtend { ty, tail, .. } => {
                self.collect_generalizable_scoped(ty, outer_level, out, bound);
                self.collect_generalizable_scoped(tail, outer_level, out, bound);
            }
            // A type-level literal is decided, so there is nothing to generalize in one.
            InferType::RowEmpty
            | InferType::Constructor(_)
            | InferType::TypeLevelString(_)
            | InferType::TypeLevelInt(_) => {}
        }
    }
}

/// Whether `variable` occurs in `ty` or in one of `constraints` outside any
/// nested `forall` that binds it. A binder the body does not mention is not
/// quantified, so a scheme never declares a quantifier nothing refers to.
fn constraint_or_type_mentions(
    ty: &InferType,
    constraints: &[ClassConstraint],
    variable: u32,
) -> bool {
    if occurs_below_forall(ty, variable) {
        return true;
    }
    constraints
        .iter()
        .flat_map(|constraint| &constraint.arguments)
        .any(|argument| occurs_below_forall(argument, variable))
}

fn occurs_below_forall(ty: &InferType, variable: u32) -> bool {
    match ty {
        InferType::Variable(other) => *other == variable,
        InferType::Application(function, argument) => {
            occurs_below_forall(function, variable) || occurs_below_forall(argument, variable)
        }
        InferType::RowExtend { ty, tail, .. } => {
            occurs_below_forall(ty, variable) || occurs_below_forall(tail, variable)
        }
        InferType::ForAll { variables, body } => {
            !variables.contains(&variable) && occurs_below_forall(body, variable)
        }
        InferType::Constrained { constraints, body } => {
            constraints
                .iter()
                .flat_map(|constraint| &constraint.arguments)
                .any(|argument| occurs_below_forall(argument, variable))
                || occurs_below_forall(body, variable)
        }
        InferType::RowEmpty
        | InferType::Constructor(_)
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => false,
    }
}
