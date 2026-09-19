use super::*;

impl Checker {
    /// Elaborates a resolved source signature into an inference type.
    ///
    /// Every type variable in a signature is rigid (universally quantified).
    /// Repeated occurrences of the same name share one inference variable, so
    /// `a -> a` is elaborated as one variable appearing twice.
    pub(super) fn elaborate_signature(&mut self, ty: &hir::Type) -> InferType {
        let mut variables = HashMap::new();
        self.elaborate_type(ty, &mut variables)
    }

    fn elaborate_type(
        &mut self,
        ty: &hir::Type,
        variables: &mut HashMap<String, InferType>,
    ) -> InferType {
        match &ty.kind {
            hir::TypeKind::Variable(name) => {
                if let Some(variable) = variables.get(name) {
                    return variable.clone();
                }
                let variable = self.fresh();
                if let InferType::Variable(id) = variable {
                    self.rigid.insert(id);
                }
                variables.insert(name.clone(), variable.clone());
                variable
            }
            hir::TypeKind::Constructor(builtin) => match builtin {
                hir::BuiltinType::Int => InferType::I32,
                hir::BuiltinType::Boolean => InferType::Boolean,
                hir::BuiltinType::String => InferType::String,
                hir::BuiltinType::Unit => InferType::Unit,
            },
            hir::TypeKind::Function { parameter, result } => InferType::Function(
                Box::new(self.elaborate_type(parameter, variables)),
                Box::new(self.elaborate_type(result, variables)),
            ),
        }
    }
}
