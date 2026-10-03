use super::*;

impl Checker {
    pub(super) fn generalize(
        &mut self,
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
        variables.sort_unstable();
        variables.dedup();
        for variable in &variables {
            self.state.generic_variables.insert(*variable);
        }
        Scheme {
            variables,
            constraints: constraints.to_vec(),
            ty: resolved,
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
