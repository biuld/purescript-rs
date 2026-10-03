use super::super::unify::substitute;
use super::super::*;

impl Checker {
    pub(in crate::typecheck) fn freshen_foralls(&mut self, ty: &InferType) -> InferType {
        match ty {
            InferType::ForAll { variables, body } => {
                let mapping = self.instantiate_type_variables(variables.iter().copied());
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
    ) -> HashMap<u32, InferType> {
        let mut type_mapping = HashMap::new();
        let mut kind_mapping = HashMap::new();
        for variable in variables {
            let kind = self.state.variable_kinds.get(&variable).cloned();
            let InferType::Variable(fresh) = self.fresh() else {
                unreachable!("fresh inference types are variables")
            };
            if let Some(kind) = kind {
                let kind = self.instantiate_kind(&kind, &mut kind_mapping);
                self.state.variable_kinds.insert(fresh, kind);
            }
            type_mapping.insert(variable, InferType::Variable(fresh));
        }
        type_mapping
    }

    fn instantiate_kind(
        &mut self,
        kind: &psrs_kind::Kind,
        mapping: &mut HashMap<u32, psrs_kind::Kind>,
    ) -> psrs_kind::Kind {
        use psrs_kind::Kind;
        match kind {
            Kind::Variable(variable) => mapping
                .entry(*variable)
                .or_insert_with(|| self.fresh_kind())
                .clone(),
            Kind::App(function, argument) => Kind::App(
                Box::new(self.instantiate_kind(function, mapping)),
                Box::new(self.instantiate_kind(argument, mapping)),
            ),
            Kind::Function(parameter, result) => Kind::Function(
                Box::new(self.instantiate_kind(parameter, mapping)),
                Box::new(self.instantiate_kind(result, mapping)),
            ),
            other => other.clone(),
        }
    }
}
