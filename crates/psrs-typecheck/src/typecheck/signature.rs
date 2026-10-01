use super::*;

impl Checker {
    /// Elaborates a signature at a use site. Its universally quantified
    /// variables must be fresh and flexible so each imported use can choose a
    /// different concrete type.
    pub(super) fn elaborate_imported_signature(&mut self, ty: &hir::Type) -> InferType {
        let (_, body) = self.elaborate_constrained_signature(ty, false);
        body
    }

    /// Elaborates an imported signature together with its class constraints.
    pub(super) fn elaborate_imported_constraints(
        &mut self,
        ty: &hir::Type,
    ) -> (Vec<ClassConstraint>, InferType) {
        self.elaborate_constrained_signature(ty, false)
    }

    /// Elaborates a declaration's signature, flattening its constraints and
    /// synthesizing one dictionary parameter per constraint in source order.
    pub(super) fn elaborate_declaration_signature(
        &mut self,
        ty: &hir::Type,
    ) -> (Vec<ClassConstraint>, Vec<(LocalId, InferType)>, InferType) {
        let (constraints, body) = self.elaborate_constrained_signature(ty, true);
        let mut parameters = Vec::with_capacity(constraints.len());
        for constraint in &constraints {
            let dictionary_type = self.dictionary_type(constraint);
            let id = LocalId(self.next_dictionary_local);
            self.next_dictionary_local += 1;
            parameters.push((id, dictionary_type));
        }
        (constraints, parameters, body)
    }

    /// Walks a signature's `forall`/`=>` spine, elaborating every constraint
    /// before its body. Constraints appear in source order.
    pub(super) fn elaborate_constrained_signature(
        &mut self,
        ty: &hir::Type,
        rigid: bool,
    ) -> (Vec<ClassConstraint>, InferType) {
        let mut variables = HashMap::new();
        self.elaborate_constraint_spine(ty, &mut variables, rigid)
    }

    fn elaborate_constraint_spine(
        &mut self,
        ty: &hir::Type,
        variables: &mut HashMap<String, InferType>,
        rigid: bool,
    ) -> (Vec<ClassConstraint>, InferType) {
        match &ty.kind {
            hir::TypeKind::Forall {
                variables: binders,
                body,
            } => {
                let mut scoped_variables = variables.clone();
                self.bind_forall_variables(binders, &mut scoped_variables, rigid);
                self.elaborate_constraint_spine(body, &mut scoped_variables, rigid)
            }
            hir::TypeKind::Constrained { constraint, body } => {
                let class = self.elaborate_constraint(constraint, variables, rigid);
                let (rest, body_ty) = self.elaborate_constraint_spine(body, variables, rigid);
                let mut constraints = Vec::new();
                if let Some(class) = class {
                    constraints.push(class);
                }
                constraints.extend(rest);
                (constraints, body_ty)
            }
            _ => (Vec::new(), self.elaborate_type_mode(ty, variables, rigid)),
        }
    }

    pub(super) fn elaborate_type(
        &mut self,
        ty: &hir::Type,
        variables: &mut HashMap<String, InferType>,
    ) -> InferType {
        self.elaborate_type_mode(ty, variables, true)
    }

    pub(super) fn elaborate_type_mode(
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
                hir::BuiltinType::Int => InferType::Constructor(TypeConstructor::Int),
                hir::BuiltinType::Number => InferType::Constructor(TypeConstructor::Number),
                hir::BuiltinType::Boolean => InferType::Constructor(TypeConstructor::Boolean),
                hir::BuiltinType::String => InferType::Constructor(TypeConstructor::String),
                hir::BuiltinType::Char => InferType::Constructor(TypeConstructor::Char),
                hir::BuiltinType::Unit => InferType::Constructor(TypeConstructor::Unit),
                hir::BuiltinType::Array => InferType::Constructor(TypeConstructor::Array),
                hir::BuiltinType::Function => InferType::Constructor(TypeConstructor::Function),
                hir::BuiltinType::Type
                | hir::BuiltinType::Constraint
                | hir::BuiltinType::Symbol
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
            hir::TypeKind::Named(id) | hir::TypeKind::Opaque(id) => {
                if self.synonyms.contains_key(id) {
                    self.expand_synonym(*id, Vec::new(), ty.span)
                } else {
                    // Foreign data stays a nominal user constructor. Opacity is
                    // `Module.opaque_ids`, not a separate type node and not `Int`.
                    InferType::Constructor(TypeConstructor::User(*id))
                }
            }
            hir::TypeKind::Application(function, argument) => {
                let (head, arguments) = flatten_spine(ty);
                if let Some(id) = nominal_type_id(head)
                    && self.synonyms.contains_key(&id)
                {
                    let arguments = arguments
                        .into_iter()
                        .map(|argument| {
                            self.elaborate_type_mode(argument, variables, rigid_variables)
                        })
                        .collect();
                    return self.expand_synonym(id, arguments, ty.span);
                }
                InferType::Application(
                    Box::new(self.elaborate_type_mode(function, variables, rigid_variables)),
                    Box::new(self.elaborate_type_mode(argument, variables, rigid_variables)),
                )
            }
            hir::TypeKind::Function { parameter, result } => arrow(
                self.elaborate_type_mode(parameter, variables, rigid_variables),
                self.elaborate_type_mode(result, variables, rigid_variables),
            ),
            hir::TypeKind::Forall {
                variables: binders,
                body,
            } => {
                let mut scoped_variables = variables.clone();
                let quantified =
                    self.bind_forall_variables(binders, &mut scoped_variables, rigid_variables);
                let body = self.elaborate_type_mode(body, &mut scoped_variables, rigid_variables);
                InferType::ForAll {
                    variables: quantified,
                    body: Box::new(body),
                }
            }
            hir::TypeKind::Constrained { constraint, body } => {
                let Some(constraint) =
                    self.elaborate_constraint(constraint, variables, rigid_variables)
                else {
                    return self.fresh();
                };
                InferType::Constrained {
                    constraints: vec![constraint],
                    body: Box::new(self.elaborate_type_mode(body, variables, rigid_variables)),
                }
            }
            hir::TypeKind::Record { fields, tail } => {
                let mut seen = HashSet::new();
                let mut elaborated = Vec::with_capacity(fields.len());
                for field in fields {
                    if !seen.insert(field.label.clone()) {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::TypeMismatch,
                            field.span,
                            format!("record label `{}` occurs more than once", field.label),
                        ));
                        continue;
                    }
                    elaborated.push((
                        field.label.clone(),
                        self.elaborate_type_mode(&field.ty, variables, rigid_variables),
                    ));
                }
                elaborated.sort_by(|left, right| left.0.cmp(&right.0));
                let tail = match tail {
                    None => InferType::RowEmpty,
                    Some(tail) => {
                        match self.elaborate_type_mode(tail, variables, rigid_variables) {
                            InferType::Variable(variable) => InferType::Variable(variable),
                            _ => {
                                self.errors.push(TypeCheckError::new(
                                    TypeCheckErrorKind::UnsupportedType,
                                    tail.span,
                                    "a record row tail must be a type variable",
                                ));
                                InferType::RowEmpty
                            }
                        }
                    }
                };
                record_type(elaborated, tail)
            }
            hir::TypeKind::Row { .. } | hir::TypeKind::Integer(_) | hir::TypeKind::String(_) => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedType,
                    ty.span,
                    "this type is not supported yet",
                ));
                self.fresh()
            }
        }
    }

    fn bind_forall_variables(
        &mut self,
        binders: &[hir::TypeParameter],
        variables: &mut HashMap<String, InferType>,
        rigid: bool,
    ) -> Vec<u32> {
        let mut kind_scope = HashMap::new();
        let mut quantified = Vec::with_capacity(binders.len());
        for binder in binders {
            let variable = self.fresh();
            let kind = binder
                .kind
                .as_ref()
                .map(|kind| self.kind_from_hir(kind, &kind_scope))
                .unwrap_or_else(|| self.fresh_kind());
            if let InferType::Variable(id) = variable {
                self.infer_variable_kinds.insert(id, kind.clone());
                if rigid {
                    self.rigid.insert(id);
                }
                quantified.push(id);
            }
            // An unannotated forall binder can itself be a kind variable; a
            // later binder annotation may refer to it.
            if binder.kind.is_none() {
                kind_scope.insert(binder.name.clone(), kind);
            }
            variables.insert(binder.name.clone(), variable);
        }
        quantified
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

pub(super) fn nominal_type_id(ty: &hir::Type) -> Option<hir::TypeId> {
    match &ty.kind {
        hir::TypeKind::Named(id) | hir::TypeKind::Opaque(id) => Some(*id),
        _ => None,
    }
}

pub(super) fn flatten_spine(ty: &hir::Type) -> (&hir::Type, Vec<&hir::Type>) {
    let mut arguments = Vec::new();
    let mut head = ty;
    while let hir::TypeKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function.as_ref();
    }
    arguments.reverse();
    (head, arguments)
}
