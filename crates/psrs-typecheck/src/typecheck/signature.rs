use super::*;

impl Checker {
    /// Elaborates a resolved source signature into an inference type.
    ///
    /// Every type variable in a signature is rigid (universally quantified).
    /// Repeated occurrences of the same name share one inference variable, so
    /// `a -> a` is elaborated as one variable appearing twice.
    pub(super) fn elaborate_signature(&mut self, ty: &hir::Type) -> InferType {
        let mut variables = HashMap::new();
        self.elaborate_type_mode(ty, &mut variables, true)
    }

    /// Elaborates a signature at a use site. Its universally quantified
    /// variables must be fresh and flexible so each imported use can choose a
    /// different concrete type.
    pub(super) fn elaborate_imported_signature(&mut self, ty: &hir::Type) -> InferType {
        let mut variables = HashMap::new();
        self.elaborate_type_mode(ty, &mut variables, false)
    }

    pub(super) fn elaborate_type(
        &mut self,
        ty: &hir::Type,
        variables: &mut HashMap<String, InferType>,
    ) -> InferType {
        self.elaborate_type_mode(ty, variables, true)
    }

    fn elaborate_type_mode(
        &mut self,
        ty: &hir::Type,
        variables: &mut HashMap<String, InferType>,
        rigid_variables: bool,
    ) -> InferType {
        match &ty.kind {
            hir::TypeKind::Variable(name) => {
                if let Some(variable) = variables.get(name) {
                    return variable.clone();
                }
                let variable = self.fresh();
                if rigid_variables && let InferType::Variable(id) = variable {
                    self.rigid.insert(id);
                }
                variables.insert(name.clone(), variable.clone());
                variable
            }
            hir::TypeKind::Constructor(builtin) => match builtin {
                hir::BuiltinType::Int => InferType::I32,
                hir::BuiltinType::Number => InferType::F64,
                hir::BuiltinType::Boolean => InferType::Boolean,
                hir::BuiltinType::String => InferType::String,
                hir::BuiltinType::Char => InferType::Char,
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
                } else if Some(*id) == self.effect_type {
                    InferType::Constructor(TypeConstructor::Effect)
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
                        .map(|argument| {
                            self.elaborate_type_mode(argument, variables, rigid_variables)
                        })
                        .collect();
                    return self.expand_synonym(*id, arguments, ty.span);
                }
                if matches!(&head.kind, hir::TypeKind::Named(id) if Some(*id) == self.effect_type) {
                    let Some(argument) = arguments.first() else {
                        return self.fresh();
                    };
                    if arguments.len() != 1 {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::UnsupportedType,
                            ty.span,
                            "Effect takes exactly one type argument",
                        ));
                        return self.fresh();
                    }
                    let argument = self.elaborate_type_mode(argument, variables, rigid_variables);
                    return if self.effect_runtime_representation {
                        InferType::Function(Box::new(InferType::I32), Box::new(argument))
                    } else {
                        InferType::Application(
                            Box::new(InferType::Constructor(TypeConstructor::Effect)),
                            Box::new(argument),
                        )
                    };
                }
                InferType::Application(
                    Box::new(self.elaborate_type_mode(function, variables, rigid_variables)),
                    Box::new(self.elaborate_type_mode(argument, variables, rigid_variables)),
                )
            }
            hir::TypeKind::Function { parameter, result } => InferType::Function(
                Box::new(self.elaborate_type_mode(parameter, variables, rigid_variables)),
                Box::new(self.elaborate_type_mode(result, variables, rigid_variables)),
            ),
            hir::TypeKind::Forall { body, .. } => {
                self.elaborate_type_mode(body, variables, rigid_variables)
            }
            hir::TypeKind::Constrained { body, .. } => {
                self.elaborate_type_mode(body, variables, rigid_variables)
            }
            hir::TypeKind::Record { fields, tail } if tail.is_none() => {
                let mut fields = fields
                    .iter()
                    .map(|field| {
                        (
                            field.label.clone(),
                            self.elaborate_type_mode(&field.ty, variables, rigid_variables),
                        )
                    })
                    .collect::<Vec<_>>();
                fields.sort_by(|left, right| left.0.cmp(&right.0));
                InferType::Record(fields)
            }
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
