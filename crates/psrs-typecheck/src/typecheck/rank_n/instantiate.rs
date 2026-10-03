use super::super::unify::substitute;
use super::super::*;

impl Checker {
    pub(in crate::typecheck) fn freshen_foralls(&mut self, ty: &InferType) -> InferType {
        match ty {
            InferType::ForAll { variables, body } => {
                let mapping =
                    self.instantiate_type_variables(variables.iter().copied(), &HashMap::new());
                let mut fresh_variables = Vec::with_capacity(variables.len());
                for variable in variables {
                    if let Some(InferType::Variable(fresh)) = mapping.get(variable) {
                        self.state.rigid.insert(*fresh);
                        if let Some(name) = self.scope.type_variable_names.get(variable).cloned() {
                            self.scope.type_variable_names.insert(*fresh, name);
                        }
                        fresh_variables.push(*fresh);
                    }
                }
                let body = substitute(body, &mapping);
                InferType::ForAll {
                    variables: fresh_variables,
                    body: Box::new(self.freshen_foralls(&body)),
                }
            }
            InferType::Constrained { constraints, body } => InferType::Constrained {
                constraints: constraints
                    .iter()
                    .map(|constraint| ClassConstraint {
                        arguments: constraint
                            .arguments
                            .iter()
                            .map(|ty| self.freshen_foralls(ty))
                            .collect(),
                        ..constraint.clone()
                    })
                    .collect(),
                body: Box::new(self.freshen_foralls(body)),
            },
            InferType::Application(function, argument) => InferType::Application(
                Box::new(self.freshen_foralls(function)),
                Box::new(self.freshen_foralls(argument)),
            ),
            InferType::RowExtend { label, ty, tail } => InferType::RowExtend {
                label: label.clone(),
                ty: Box::new(self.freshen_foralls(ty)),
                tail: Box::new(self.freshen_foralls(tail)),
            },
            other => other.clone(),
        }
    }

    pub(in crate::typecheck) fn instantiate_type_variables(
        &mut self,
        variables: impl IntoIterator<Item = u32>,
        kinds: &HashMap<u32, Kind>,
    ) -> HashMap<u32, InferType> {
        let mut type_mapping = HashMap::new();
        let mut kind_mapping = HashMap::new();
        for variable in variables {
            // The scheme's own record of the variable's kind comes first, so a
            // scheme carries its polymorphism; the solver table is the fallback
            // for a variable that reached inference before schemes recorded
            // kinds, such as one a structural `forall` binder introduced.
            let kind = kinds
                .get(&variable)
                .cloned()
                .or_else(|| self.recorded_kind(variable));
            let InferType::Variable(fresh) = self.fresh() else {
                unreachable!("fresh inference types are variables")
            };
            if let Some(kind) = kind {
                let kind = self.instantiate_kind(kind, &mut kind_mapping);
                self.record_variable_kind(fresh, kind);
            }
            type_mapping.insert(variable, InferType::Variable(fresh));
        }
        type_mapping
    }

    /// Replaces the kind variables of one instantiation with fresh ones. The
    /// mapping is shared across every binder of the same instantiation, so a
    /// kind variable occurring in two of them stays one variable, and the
    /// replacement itself is `psrs-kind`'s substitution rather than a walk of
    /// the kind language here.
    fn instantiate_kind(&mut self, kind: Kind, mapping: &mut HashMap<u32, Kind>) -> Kind {
        let mut quantified = Vec::new();
        collect_kind_variables(&kind, &mut quantified);
        for variable in quantified {
            let fresh = self.fresh_kind();
            mapping.insert(variable, fresh);
        }
        psrs_kind::substitute(&kind, mapping)
    }
}

/// Every kind variable occurring in `kind`, in the order it is met.
fn collect_kind_variables(kind: &Kind, out: &mut Vec<u32>) {
    match kind {
        Kind::Variable(variable) => {
            if !out.contains(variable) {
                out.push(*variable);
            }
        }
        Kind::App(function, argument) => {
            collect_kind_variables(function, out);
            collect_kind_variables(argument, out);
        }
        Kind::Function(parameter, result) => {
            collect_kind_variables(parameter, out);
            collect_kind_variables(result, out);
        }
        Kind::Builtin(_) | Kind::Named(_) => {}
    }
}
