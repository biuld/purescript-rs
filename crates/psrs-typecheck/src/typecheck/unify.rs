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
            (InferType::Constructor(a), InferType::Constructor(b)) if a == b => {}
            // A trusted effect-library definition applies an `Effect a` value to
            // its hidden context parameter. At the source level that is an
            // application of an effect to an integer context, so unify the
            // effect's result with the application's result. Ordinary modules
            // never reach this arm because they keep `Effect` nominal and never
            // name the context. The expected arrow is itself an application
            // spine whose inner node is the partial application.
            (InferType::Application(function, argument), InferType::Application(inner, result))
            | (InferType::Application(inner, result), InferType::Application(function, argument))
                if self.effect_runtime_representation
                    && matches!(*function, InferType::Constructor(TypeConstructor::Effect))
                    && matches!(*inner, InferType::Application(_, _)) =>
            {
                self.unify(*argument, *result, span);
            }
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
                TypeConstructor::Effect => "Effect".into(),
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
            other => other,
        }
    }

    fn adjust_levels(&mut self, ty: &InferType, max_level: u32) {
        match ty {
            InferType::Variable(variable) => {
                if let Some(level) = self.levels.get_mut(variable)
                    && *level > max_level
                {
                    *level = max_level;
                }
            }
            InferType::Application(function, argument) => {
                self.adjust_levels(function, max_level);
                self.adjust_levels(argument, max_level);
            }
            InferType::RowExtend { ty, tail, .. } => {
                self.adjust_levels(ty, max_level);
                self.adjust_levels(tail, max_level);
            }
            InferType::RowEmpty | InferType::Constructor(_) => {}
        }
    }

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
            self.generic_variables.insert(*variable);
        }
        Scheme {
            variables,
            constraints: constraints.to_vec(),
            ty: resolved,
        }
    }

    fn collect_generalizable(&self, ty: &InferType, outer_level: u32, out: &mut Vec<u32>) {
        match ty {
            InferType::Variable(variable) => {
                if self.levels.get(variable).copied().unwrap_or(TOP_LEVEL) > outer_level {
                    out.push(*variable);
                }
            }
            InferType::Application(function, argument) => {
                self.collect_generalizable(function, outer_level, out);
                self.collect_generalizable(argument, outer_level, out);
            }
            InferType::RowExtend { ty, tail, .. } => {
                self.collect_generalizable(ty, outer_level, out);
                self.collect_generalizable(tail, outer_level, out);
            }
            InferType::RowEmpty | InferType::Constructor(_) => {}
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
            InferType::Constructor(TypeConstructor::Effect) => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedType,
                    span,
                    "Effect must be applied to exactly one type argument",
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
                    TypeConstructor::Effect => unreachable!("handled above"),
                    TypeConstructor::User(id) => thir::TypeConstructor::User(id),
                })))
            }
            InferType::Application(function, argument) => {
                if matches!(*function, InferType::Constructor(TypeConstructor::Record)) {
                    let row = self.finalize_row(*argument, span, interner, generics)?;
                    let head = interner.intern(Type::Constructor(thir::TypeConstructor::Record));
                    return Some(interner.intern(Type::Application(head, row)));
                }
                if matches!(
                    self.resolve_type(*function.clone()),
                    InferType::Constructor(TypeConstructor::Effect)
                ) {
                    // Keep the imported opaque `Effect` identity in Core. The
                    // backend selects its closure representation after checking;
                    // the execution context is a hidden parameter of that
                    // representation, never a Core type.
                    let Some(effect_type) = self.effect_type else {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::UnsupportedType,
                            span,
                            "`Effect` is not available in this module",
                        ));
                        return None;
                    };
                    let result = self.finalize_type(&argument, span, interner, generics)?;
                    let constructor = interner
                        .intern(Type::Constructor(thir::TypeConstructor::User(effect_type)));
                    return Some(interner.intern(Type::Application(constructor, result)));
                }
                let function = self.finalize_type(&function, span, interner, generics);
                let argument = self.finalize_type(&argument, span, interner, generics);
                Some(interner.intern(Type::Application(function?, argument?)))
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
        InferType::RowExtend { label, ty, tail } => InferType::RowExtend {
            label: label.clone(),
            ty: Box::new(substitute(ty, mapping)),
            tail: Box::new(substitute(tail, mapping)),
        },
        InferType::RowEmpty => InferType::RowEmpty,
        other => other.clone(),
    }
}
