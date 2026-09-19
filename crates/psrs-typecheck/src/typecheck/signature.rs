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
                hir::BuiltinType::Array => InferType::Constructor(TypeConstructor::Array),
                hir::BuiltinType::Type
                | hir::BuiltinType::Constraint
                | hir::BuiltinType::Symbol
                | hir::BuiltinType::Function
                | hir::BuiltinType::Row
                | hir::BuiltinType::Record => {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::UnsupportedType,
                        ty.span,
                        "this type is not supported yet",
                    ));
                    self.fresh()
                }
            },
            hir::TypeKind::Named(id) => {
                if self.synonyms.contains_key(id) {
                    self.expand_synonym(*id, Vec::new(), ty.span)
                } else {
                    InferType::Constructor(TypeConstructor::User(*id))
                }
            }
            hir::TypeKind::Application(function, argument) => {
                let (head, arguments) = flatten_spine(ty);
                if let hir::TypeKind::Named(id) = &head.kind
                    && self.synonyms.contains_key(id)
                {
                    let arguments = arguments
                        .into_iter()
                        .map(|argument| self.elaborate_type(argument, variables))
                        .collect();
                    return self.expand_synonym(*id, arguments, ty.span);
                }
                InferType::Application(
                    Box::new(self.elaborate_type(function, variables)),
                    Box::new(self.elaborate_type(argument, variables)),
                )
            }
            hir::TypeKind::Function { parameter, result } => InferType::Function(
                Box::new(self.elaborate_type(parameter, variables)),
                Box::new(self.elaborate_type(result, variables)),
            ),
            hir::TypeKind::Forall { body, .. } => self.elaborate_type(body, variables),
            hir::TypeKind::Constrained { body, .. } => self.elaborate_type(body, variables),
            hir::TypeKind::Row { .. }
            | hir::TypeKind::Record { .. }
            | hir::TypeKind::Integer(_)
            | hir::TypeKind::String(_) => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedType,
                    ty.span,
                    "this type is not supported yet",
                ));
                self.fresh()
            }
        }
    }

    /// Expands a type synonym application by substituting the elaborated
    /// arguments for the synonym's parameters. A recursive synonym, which the
    /// kind pass rejects, is reported here rather than looping.
    fn expand_synonym(
        &mut self,
        id: hir::TypeId,
        arguments: Vec<InferType>,
        span: TextRange,
    ) -> InferType {
        let Some(synonym) = self.synonyms.get(&id).cloned() else {
            return InferType::Constructor(TypeConstructor::User(id));
        };
        if arguments.len() != synonym.parameters.len() {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedType,
                span,
                "a type synonym must be fully applied",
            ));
            return self.fresh();
        }
        if !self.expanding.insert(id) {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedType,
                span,
                "a type synonym may not be recursive",
            ));
            return self.fresh();
        }
        let mut locals = HashMap::new();
        for (parameter, argument) in synonym.parameters.iter().zip(arguments) {
            locals.insert(parameter.clone(), argument);
        }
        let expanded = self.elaborate_type(&synonym.body, &mut locals);
        self.expanding.remove(&id);
        expanded
    }
}

fn flatten_spine(ty: &hir::Type) -> (&hir::Type, Vec<&hir::Type>) {
    let mut arguments = Vec::new();
    let mut head = ty;
    while let hir::TypeKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function.as_ref();
    }
    arguments.reverse();
    (head, arguments)
}
