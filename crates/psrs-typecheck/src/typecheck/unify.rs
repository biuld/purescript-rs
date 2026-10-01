use super::*;

impl Checker {
    pub(super) fn fresh(&mut self) -> InferType {
        let id = self.next_variable;
        self.next_variable += 1;
        self.levels.insert(id, self.level);
        let kind = self.next_kind_variable;
        self.next_kind_variable += 1;
        self.infer_variable_kinds
            .insert(id, psrs_kind::Kind::Variable(kind));
        InferType::Variable(id)
    }

    pub(super) fn unify(&mut self, left: InferType, right: InferType, span: TextRange) {
        let left = self.resolve_type(left);
        let right = self.resolve_type(right);
        match (left, right) {
            (InferType::Variable(a), InferType::Variable(b)) if a == b => {}
            (InferType::Variable(a), InferType::Variable(b)) => {
                match (self.rigid.contains(&a), self.rigid.contains(&b)) {
                    (true, true) => self.signature_mismatch(
                        InferType::Variable(a),
                        InferType::Variable(b),
                        span,
                    ),
                    (true, false) => {
                        self.bind_variable(b, InferType::Variable(a), span);
                    }
                    (false, _) => {
                        self.bind_variable(a, InferType::Variable(b), span);
                    }
                }
            }
            (InferType::Variable(variable), ty) if self.rigid.contains(&variable) => {
                self.signature_mismatch(InferType::Variable(variable), ty, span);
            }
            (ty, InferType::Variable(variable)) if self.rigid.contains(&variable) => {
                self.signature_mismatch(ty, InferType::Variable(variable), span);
            }
            (InferType::Variable(variable), ty) | (ty, InferType::Variable(variable)) => {
                self.bind_variable(variable, ty, span);
            }
            (
                InferType::ForAll {
                    variables: left_variables,
                    body: left_body,
                },
                InferType::ForAll {
                    variables: right_variables,
                    body: right_body,
                },
            ) if left_variables.len() == right_variables.len() => {
                let mapping = right_variables
                    .into_iter()
                    .zip(left_variables)
                    .map(|(right, left)| (right, InferType::Variable(left)))
                    .collect();
                self.unify(*left_body, substitute(&right_body, &mapping), span);
            }
            (
                InferType::Constrained {
                    constraints: left_constraints,
                    body: left_body,
                },
                InferType::Constrained {
                    constraints: right_constraints,
                    body: right_body,
                },
            ) if left_constraints.len() == right_constraints.len()
                && left_constraints
                    .iter()
                    .zip(&right_constraints)
                    .all(|(left, right)| {
                        left.class_id == right.class_id
                            && left.arguments.len() == right.arguments.len()
                    }) =>
            {
                for (left, right) in left_constraints.iter().zip(&right_constraints) {
                    for (left, right) in left.arguments.iter().zip(&right.arguments) {
                        self.unify(left.clone(), right.clone(), span);
                    }
                }
                self.unify(*left_body, *right_body, span);
            }
            (InferType::Constructor(a), InferType::Constructor(b)) if a == b => {}
            (InferType::Application(f1, a1), InferType::Application(f2, a2))
                if matches!(*f1, InferType::Constructor(TypeConstructor::Record))
                    && matches!(*f2, InferType::Constructor(TypeConstructor::Record)) =>
            {
                self.unify_rows(*a1, *a2, span);
            }
            (InferType::Application(f1, a1), InferType::Application(f2, a2)) => {
                self.unify(*f1, *f2, span);
                self.unify(*a1, *a2, span);
            }
            (expected, actual) => {
                let expected = self.display_type(&expected);
                let actual = self.display_type(&actual);
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::TypeMismatch,
                    span,
                    format!("type mismatch: expected {expected}, found {actual}"),
                ));
            }
        }
    }

    /// Binds `variable` to `ty`, reporting an occurs-check failure and leaving
    /// the substitution unchanged when it would be recursive. Returns whether a
    /// binding was recorded, so a fixed-point caller can detect progress.
    pub(super) fn bind_variable(&mut self, variable: u32, ty: InferType, span: TextRange) -> bool {
        if occurs(variable, &ty) {
            let displayed = self.display_type(&ty);
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::OccursCheck,
                span,
                format!("infinite type: _T{variable} occurs in {displayed}"),
            ));
            false
        } else if self.reject_skolem_escape(variable, &ty, span) {
            false
        } else {
            let level = self.levels.get(&variable).copied().unwrap_or(TOP_LEVEL);
            self.adjust_levels(&ty, level);
            self.substitutions.insert(variable, ty);
            true
        }
    }

    fn signature_mismatch(&mut self, expected: InferType, found: InferType, span: TextRange) {
        let expected_display = self.display_type(&expected);
        let found_display = self.display_type(&found);
        self.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::TypeMismatch,
            span,
            format!("signature mismatch: expected {expected_display}, found {found_display}"),
        ));
    }

    /// Renders an inference type for diagnostics, using declared type names.
    pub(super) fn display_type(&self, ty: &InferType) -> String {
        match self.resolve_type(ty.clone()) {
            InferType::Variable(variable) => format!("_T{variable}"),
            InferType::Constructor(constructor) => match constructor {
                TypeConstructor::Function => "Function".into(),
                TypeConstructor::Record => "Record".into(),
                TypeConstructor::Array => "Array".into(),
                TypeConstructor::Int => "Int".into(),
                TypeConstructor::Number => "Number".into(),
                TypeConstructor::Boolean => "Boolean".into(),
                TypeConstructor::String => "String".into(),
                TypeConstructor::Char => "Char".into(),
                TypeConstructor::Unit => "Unit".into(),
                TypeConstructor::User(id) => self
                    .type_names
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| format!("Type#{}.{}", id.module.0, id.index)),
            },
            InferType::Application(function, argument)
                if matches!(*function, InferType::Constructor(TypeConstructor::Record)) =>
            {
                self.display_record(&argument)
            }
            InferType::Application(function, argument) => {
                if let Some((parameter, result)) = infer_arrow_parts(&function, &argument) {
                    format!(
                        "({} -> {})",
                        self.display_type(&parameter),
                        self.display_type(&result)
                    )
                } else {
                    format!(
                        "({} {})",
                        self.display_type(&function),
                        self.display_type(&argument)
                    )
                }
            }
            InferType::ForAll { variables, body } => format!(
                "(forall {}. {})",
                variables
                    .iter()
                    .map(|variable| format!("_T{variable}"))
                    .collect::<Vec<_>>()
                    .join(" "),
                self.display_type(&body)
            ),
            InferType::Constrained { constraints, body } => format!(
                "({} => {})",
                constraints
                    .iter()
                    .map(|constraint| self
                        .display_constraint(constraint.class_id, &constraint.arguments))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.display_type(&body)
            ),
            InferType::RowEmpty => "{ }".into(),
            row @ InferType::RowExtend { .. } => self.display_record(&row),
        }
    }

    fn display_record(&self, row: &InferType) -> String {
        let FlatRow { fields, tail } = self.flatten_row(row.clone());
        let rendered = fields
            .iter()
            .map(|(label, ty)| format!("{label}: {}", self.display_type(ty)))
            .collect::<Vec<_>>()
            .join(", ");
        match tail {
            RowTail::Closed => format!("{{{rendered}}}"),
            RowTail::Open(variable) => {
                if rendered.is_empty() {
                    format!("{{ | _T{variable} }}")
                } else {
                    format!("{{{rendered} | _T{variable}}}")
                }
            }
        }
    }

    pub(super) fn resolve_type(&self, ty: InferType) -> InferType {
        match ty {
            InferType::Variable(variable) => self
                .substitutions
                .get(&variable)
                .map(|ty| self.resolve_type(ty.clone()))
                .unwrap_or(InferType::Variable(variable)),
            InferType::Application(function, argument) => InferType::Application(
                Box::new(self.resolve_type(*function)),
                Box::new(self.resolve_type(*argument)),
            ),
            InferType::RowExtend { label, ty, tail } => InferType::RowExtend {
                label,
                ty: Box::new(self.resolve_type(*ty)),
                tail: Box::new(self.resolve_type(*tail)),
            },
            InferType::ForAll { variables, body } => InferType::ForAll {
                variables,
                body: Box::new(self.resolve_type(*body)),
            },
            InferType::Constrained { constraints, body } => InferType::Constrained {
                constraints: constraints
                    .into_iter()
                    .map(|constraint| ClassConstraint {
                        arguments: constraint
                            .arguments
                            .into_iter()
                            .map(|ty| self.resolve_type(ty))
                            .collect(),
                        ..constraint
                    })
                    .collect(),
                body: Box::new(self.resolve_type(*body)),
            },
            other => other,
        }
    }

    fn adjust_levels(&mut self, ty: &InferType, max_level: u32) {
        self.adjust_levels_excluding(ty, max_level, &HashSet::new());
    }

    fn adjust_levels_excluding(&mut self, ty: &InferType, max_level: u32, bound: &HashSet<u32>) {
        match ty {
            InferType::Variable(variable) if !bound.contains(variable) => {
                if let Some(level) = self.levels.get_mut(variable)
                    && *level > max_level
                {
                    *level = max_level;
                }
            }
            InferType::Variable(_) | InferType::Constructor(_) | InferType::RowEmpty => {}
            InferType::Application(function, argument) => {
                self.adjust_levels_excluding(function, max_level, bound);
                self.adjust_levels_excluding(argument, max_level, bound);
            }
            InferType::ForAll { variables, body } => {
                let mut bound = bound.clone();
                bound.extend(variables.iter().copied());
                self.adjust_levels_excluding(body, max_level, &bound);
            }
            InferType::Constrained { constraints, body } => {
                for argument in constraints
                    .iter()
                    .flat_map(|constraint| &constraint.arguments)
                {
                    self.adjust_levels_excluding(argument, max_level, bound);
                }
                self.adjust_levels_excluding(body, max_level, bound);
            }
            InferType::RowExtend { ty, tail, .. } => {
                self.adjust_levels_excluding(ty, max_level, bound);
                self.adjust_levels_excluding(tail, max_level, bound);
            }
        }
    }

    pub(super) fn finalize_type(
        &mut self,
        ty: &InferType,
        span: TextRange,
        interner: &mut TypeInterner,
        generics: &HashSet<u32>,
    ) -> Option<TypeId> {
        match self.resolve_type(ty.clone()) {
            InferType::Variable(variable) if generics.contains(&variable) => {
                Some(interner.intern(Type::Variable(TypeVariableId(variable))))
            }
            InferType::Variable(variable) => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnconstrainedType,
                    span,
                    format!("cannot infer a monomorphic type for _T{variable}"),
                ));
                None
            }
            InferType::Constructor(constructor) => {
                Some(interner.intern(Type::Constructor(match constructor {
                    TypeConstructor::Function => thir::TypeConstructor::Function,
                    TypeConstructor::Record => thir::TypeConstructor::Record,
                    TypeConstructor::Array => thir::TypeConstructor::Array,
                    TypeConstructor::Int => thir::TypeConstructor::Int,
                    TypeConstructor::Number => thir::TypeConstructor::Number,
                    TypeConstructor::Boolean => thir::TypeConstructor::Boolean,
                    TypeConstructor::String => thir::TypeConstructor::String,
                    TypeConstructor::Char => thir::TypeConstructor::Char,
                    TypeConstructor::Unit => thir::TypeConstructor::Unit,
                    TypeConstructor::User(id) => thir::TypeConstructor::User(id),
                })))
            }
            InferType::Application(function, argument) => {
                if matches!(*function, InferType::Constructor(TypeConstructor::Record)) {
                    let row = self.finalize_row(*argument, span, interner, generics)?;
                    let head = interner.intern(Type::Constructor(thir::TypeConstructor::Record));
                    return Some(interner.intern(Type::Application(head, row)));
                }
                let function = self.finalize_type(&function, span, interner, generics);
                let argument = self.finalize_type(&argument, span, interner, generics);
                Some(interner.intern(Type::Application(function?, argument?)))
            }
            InferType::ForAll { variables, body } => {
                let mut scoped_generics = generics.clone();
                scoped_generics.extend(variables.iter().copied());
                let body = self.finalize_type(&body, span, interner, &scoped_generics)?;
                Some(interner.intern(Type::ForAll {
                    variables: variables.into_iter().map(TypeVariableId).collect(),
                    body,
                }))
            }
            InferType::Constrained { constraints, body } => {
                let mut result = self.finalize_type(&body, span, interner, generics)?;
                for constraint in constraints.iter().rev() {
                    let dictionary = self.dictionary_type(constraint);
                    let parameter = self.finalize_type(&dictionary, span, interner, generics)?;
                    let function =
                        interner.intern(Type::Constructor(thir::TypeConstructor::Function));
                    let function = interner.intern(Type::Application(function, parameter));
                    result = interner.intern(Type::Application(function, result));
                }
                Some(result)
            }
            InferType::RowEmpty => Some(interner.intern(Type::RowEmpty)),
            row @ InferType::RowExtend { .. } => self.finalize_row(row, span, interner, generics),
        }
    }
}

pub(super) fn substitute(ty: &InferType, mapping: &HashMap<u32, InferType>) -> InferType {
    match ty {
        InferType::Variable(variable) => mapping
            .get(variable)
            .cloned()
            .unwrap_or(InferType::Variable(*variable)),
        InferType::Application(function, argument) => InferType::Application(
            Box::new(substitute(function, mapping)),
            Box::new(substitute(argument, mapping)),
        ),
        InferType::ForAll { variables, body } => {
            let mut mapping = mapping.clone();
            for variable in variables {
                mapping.remove(variable);
            }
            InferType::ForAll {
                variables: variables.clone(),
                body: Box::new(substitute(body, &mapping)),
            }
        }
        InferType::Constrained { constraints, body } => InferType::Constrained {
            constraints: constraints
                .iter()
                .map(|constraint| ClassConstraint {
                    arguments: constraint
                        .arguments
                        .iter()
                        .map(|ty| substitute(ty, mapping))
                        .collect(),
                    ..constraint.clone()
                })
                .collect(),
            body: Box::new(substitute(body, mapping)),
        },
        InferType::RowExtend { label, ty, tail } => InferType::RowExtend {
            label: label.clone(),
            ty: Box::new(substitute(ty, mapping)),
            tail: Box::new(substitute(tail, mapping)),
        },
        InferType::RowEmpty => InferType::RowEmpty,
        other => other.clone(),
    }
}
