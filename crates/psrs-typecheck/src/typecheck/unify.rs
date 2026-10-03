use super::*;

impl Checker {
    pub(super) fn fresh(&mut self) -> InferType {
        let id = self.state.next_variable;
        self.state.next_variable += 1;
        self.state.levels.insert(id, self.state.level);
        // Every type unknown carries a kind from the moment it exists, so the
        // binding check always has one to compare against.
        let kind = self.fresh_kind();
        self.record_variable_kind(id, kind);
        InferType::Variable(id)
    }

    pub(super) fn unify(&mut self, left: InferType, right: InferType, span: TextRange) {
        let left = self.resolve_type(left);
        let right = self.resolve_type(right);
        match (left, right) {
            (InferType::Variable(a), InferType::Variable(b)) if a == b => {}
            (InferType::Variable(a), InferType::Variable(b)) => {
                match (self.state.rigid.contains(&a), self.state.rigid.contains(&b)) {
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
            (InferType::Variable(variable), ty) if self.state.rigid.contains(&variable) => {
                self.signature_mismatch(InferType::Variable(variable), ty, span);
            }
            (ty, InferType::Variable(variable)) if self.state.rigid.contains(&variable) => {
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
            // Two decided literals unify when their scalar sequences or values
            // are equal. A literal is never bound to anything: the arms above
            // already solve an unknown variable *to* the literal, so this only
            // decides the case where both sides are literals.
            (InferType::TypeLevelString(a), InferType::TypeLevelString(b)) if a == b => {}
            (InferType::TypeLevelInt(a), InferType::TypeLevelInt(b)) if a == b => {}
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
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::TypeMismatch,
                    span,
                    format!("type mismatch: expected {expected}, found {actual}"),
                ));
            }
        }
    }

    /// Binds `variable` to `ty`, rejecting a binding that would be recursive,
    /// would let a skolem escape, or would give the variable a kind its recorded
    /// kind does not admit.
    ///
    /// Returns whether a binding was recorded, so a fixed-point caller can
    /// detect progress. The kind check is part of the same operation as the
    /// occurs, escape, and level rules: whether an operation is kind-corrected
    /// must not depend on which module reached it.
    pub(super) fn bind_variable(&mut self, variable: u32, ty: InferType, span: TextRange) -> bool {
        if occurs(variable, &ty) {
            let displayed = self.display_type(&ty);
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::OccursCheck,
                span,
                format!("infinite type: _T{variable} occurs in {displayed}"),
            ));
            return false;
        }
        if self.reject_skolem_escape(variable, &ty, span) {
            return false;
        }
        let level = self
            .state
            .levels
            .get(&variable)
            .copied()
            .unwrap_or(TOP_LEVEL);
        self.adjust_levels(&ty, level);
        if !self.check_binding_kind(variable, &ty, span) {
            return false;
        }
        self.state.substitutions.insert(variable, ty);
        true
    }

    /// Unifies the kind recorded for `variable` with the kind of the type it is
    /// being bound to, through the one kind solver.
    ///
    /// This is the inference-side rule `kinds.md` states: a row-valued binding
    /// is checked against `Row k`, an arrow against `Type -> Type`, and every
    /// other type against the kind its head and arguments give it. A type whose
    /// kind the checked environment does not supply is left alone, because the
    /// missing scheme is the kind pass's diagnostic and inference must not
    /// reject the same module a second time for it.
    fn check_binding_kind(&mut self, variable: u32, ty: &InferType, span: TextRange) -> bool {
        let Some(recorded) = self.recorded_kind(variable) else {
            return true;
        };
        let Some(ty_kind) = self.kind_of_type(ty, span) else {
            return true;
        };
        self.unify_kind(recorded, ty_kind, span)
    }

    fn signature_mismatch(&mut self, expected: InferType, found: InferType, span: TextRange) {
        let expected_display = self.display_type(&expected);
        let found_display = self.display_type(&found);
        self.state.errors.push(TypeCheckError::new(
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
                TypeConstructor::Row => "Row".into(),
                TypeConstructor::Array => "Array".into(),
                TypeConstructor::Int => "Int".into(),
                TypeConstructor::Number => "Number".into(),
                TypeConstructor::Boolean => "Boolean".into(),
                TypeConstructor::String => "String".into(),
                TypeConstructor::Char => "Char".into(),
                TypeConstructor::Unit => "Unit".into(),
                TypeConstructor::User(id) => self
                    .env
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
            InferType::TypeLevelString(value) => format!("\"{value}\""),
            InferType::TypeLevelInt(value) => value.to_string(),
        }
    }

    /// Renders the labelled entries of a row, in the order given.
    pub(super) fn display_row_fields(&self, fields: &[(String, InferType)]) -> String {
        fields
            .iter()
            .map(|(label, ty)| format!("{label}: {}", self.display_type(ty)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Renders a row type for a diagnostic. A value that reached this path
    /// without being a row is rendered between angle brackets, so the rendering
    /// shows the shape instead of silently claiming a closed row.
    fn display_record(&self, row: &InferType) -> String {
        // A rendering has no expression of its own to name, so an invalid shape
        // is reported at an empty range; the containing diagnostic carries the
        // span that matters.
        match self.normalize_row(row.clone(), TextRange::new(0, 0)) {
            Err(error) => {
                let fields = self.display_row_fields(&error.fields);
                let found = self.display_type(&error.found);
                if fields.is_empty() {
                    format!("<{found}>")
                } else {
                    format!("{{{fields} | <{found}>}}")
                }
            }
            Ok(FlatRow { fields, tail }) => {
                let rendered = self.display_row_fields(&fields);
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
        }
    }

    pub(super) fn resolve_type(&self, ty: InferType) -> InferType {
        match ty {
            InferType::Variable(variable) => self
                .state
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
                if let Some(level) = self.state.levels.get_mut(variable)
                    && *level > max_level
                {
                    *level = max_level;
                }
            }
            InferType::Variable(_)
            | InferType::Constructor(_)
            | InferType::RowEmpty
            | InferType::TypeLevelString(_)
            | InferType::TypeLevelInt(_) => {}
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
                self.state.errors.push(TypeCheckError::new(
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
                    TypeConstructor::Row => thir::TypeConstructor::Row,
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
            InferType::TypeLevelString(value) => {
                Some(interner.intern(Type::TypeLevelString(value)))
            }
            InferType::TypeLevelInt(value) => Some(interner.intern(Type::TypeLevelInt(value))),
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
