use super::*;

impl Checker {
    pub(super) fn fresh(&mut self) -> InferType {
        let id = self.next_variable;
        self.next_variable += 1;
        self.levels.insert(id, self.level);
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
                    (true, false) => self.bind_variable(b, InferType::Variable(a), span),
                    (false, _) => self.bind_variable(a, InferType::Variable(b), span),
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
            (InferType::I32, InferType::I32)
            | (InferType::F64, InferType::F64)
            | (InferType::Boolean, InferType::Boolean)
            | (InferType::String, InferType::String)
            | (InferType::Char, InferType::Char)
            | (InferType::Unit, InferType::Unit) => {}
            (InferType::Constructor(a), InferType::Constructor(b)) if a == b => {}
            (InferType::Application(f1, a1), InferType::Application(f2, a2)) => {
                self.unify(*f1, *f2, span);
                self.unify(*a1, *a2, span);
            }
            (InferType::Record(left), InferType::Record(right))
                if left.len() == right.len()
                    && left.iter().zip(&right).all(|((a, _), (b, _))| a == b) =>
            {
                for ((_, left), (_, right)) in left.into_iter().zip(right) {
                    self.unify(left, right, span);
                }
            }
            (InferType::Function(a1, r1), InferType::Function(a2, r2)) => {
                self.unify(*a1, *a2, span);
                self.unify(*r1, *r2, span);
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

    fn bind_variable(&mut self, variable: u32, ty: InferType, span: TextRange) {
        if occurs(variable, &ty) {
            let displayed = self.display_type(&ty);
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::OccursCheck,
                span,
                format!("infinite type: _T{variable} occurs in {displayed}"),
            ));
        } else {
            let level = self.levels.get(&variable).copied().unwrap_or(TOP_LEVEL);
            self.adjust_levels(&ty, level);
            self.substitutions.insert(variable, ty);
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
            InferType::I32 => "Int".into(),
            InferType::F64 => "Number".into(),
            InferType::Boolean => "Boolean".into(),
            InferType::String => "String".into(),
            InferType::Char => "Char".into(),
            InferType::Unit => "Unit".into(),
            InferType::Constructor(TypeConstructor::Array) => "Array".into(),
            InferType::Constructor(TypeConstructor::Effect) => "Effect".into(),
            InferType::Constructor(TypeConstructor::User(id)) => self
                .type_names
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("Type#{}.{}", id.module.0, id.index)),
            InferType::Application(function, argument) => {
                format!(
                    "({} {})",
                    self.display_type(&function),
                    self.display_type(&argument)
                )
            }
            InferType::Record(fields) => {
                let fields = fields
                    .iter()
                    .map(|(label, ty)| format!("{label}: {}", self.display_type(ty)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{fields}}}")
            }
            InferType::Function(parameter, result) => format!(
                "({} -> {})",
                self.display_type(&parameter),
                self.display_type(&result)
            ),
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
            InferType::Record(fields) => InferType::Record(
                fields
                    .into_iter()
                    .map(|(label, ty)| (label, self.resolve_type(ty)))
                    .collect(),
            ),
            InferType::Function(parameter, result) => InferType::Function(
                Box::new(self.resolve_type(*parameter)),
                Box::new(self.resolve_type(*result)),
            ),
            primitive => primitive,
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
            InferType::Application(function, argument)
            | InferType::Function(function, argument) => {
                self.adjust_levels(function, max_level);
                self.adjust_levels(argument, max_level);
            }
            InferType::Record(fields) => {
                for (_, field) in fields {
                    self.adjust_levels(field, max_level);
                }
            }
            InferType::I32
            | InferType::F64
            | InferType::Boolean
            | InferType::String
            | InferType::Char
            | InferType::Unit
            | InferType::Constructor(_) => {}
        }
    }

    pub(super) fn instantiate(&mut self, scheme: &Scheme) -> InferType {
        if scheme.variables.is_empty() {
            return scheme.ty.clone();
        }
        let mut mapping = HashMap::new();
        for variable in &scheme.variables {
            mapping.insert(*variable, self.fresh());
        }
        substitute(&scheme.ty, &mapping)
    }

    pub(super) fn generalize(&mut self, ty: &InferType, outer_level: u32) -> Scheme {
        let resolved = self.resolve_type(ty.clone());
        let mut variables = Vec::new();
        self.collect_generalizable(&resolved, outer_level, &mut variables);
        variables.sort_unstable();
        variables.dedup();
        for variable in &variables {
            self.generic_variables.insert(*variable);
        }
        Scheme {
            variables,
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
            InferType::Application(function, argument)
            | InferType::Function(function, argument) => {
                self.collect_generalizable(function, outer_level, out);
                self.collect_generalizable(argument, outer_level, out);
            }
            InferType::Record(fields) => {
                for (_, field) in fields {
                    self.collect_generalizable(field, outer_level, out);
                }
            }
            InferType::I32
            | InferType::F64
            | InferType::Boolean
            | InferType::String
            | InferType::Char
            | InferType::Unit
            | InferType::Constructor(_) => {}
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
            InferType::I32 => Some(interner.intern(Type::I32)),
            InferType::F64 => Some(interner.intern(Type::F64)),
            InferType::Boolean => Some(interner.intern(Type::Boolean)),
            InferType::String => Some(interner.intern(Type::String)),
            InferType::Char => Some(interner.intern(Type::Char)),
            InferType::Unit => Some(interner.intern(Type::Unit)),
            InferType::Constructor(TypeConstructor::Array) => {
                Some(interner.intern(Type::Constructor(thir::TypeConstructor::Array)))
            }
            InferType::Constructor(TypeConstructor::Effect) => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedType,
                    span,
                    "Effect must be applied to exactly one type argument",
                ));
                None
            }
            InferType::Constructor(TypeConstructor::User(id)) => {
                Some(interner.intern(Type::Constructor(thir::TypeConstructor::User(id))))
            }
            InferType::Application(function, argument) => {
                if matches!(
                    self.resolve_type(*function.clone()),
                    InferType::Constructor(TypeConstructor::Effect)
                ) {
                    let parameter = interner.intern(Type::I32);
                    let result = self.finalize_type(&argument, span, interner, generics)?;
                    return Some(interner.intern(Type::Function { parameter, result }));
                }
                let function = self.finalize_type(&function, span, interner, generics);
                let argument = self.finalize_type(&argument, span, interner, generics);
                Some(interner.intern(Type::Application(function?, argument?)))
            }
            InferType::Record(fields) => {
                let fields = fields
                    .into_iter()
                    .map(|(label, field)| {
                        Some((label, self.finalize_type(&field, span, interner, generics)?))
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(interner.intern(Type::Record(fields)))
            }
            InferType::Function(parameter, result) => {
                let parameter = self.finalize_type(&parameter, span, interner, generics);
                let result = self.finalize_type(&result, span, interner, generics);
                Some(interner.intern(Type::Function {
                    parameter: parameter?,
                    result: result?,
                }))
            }
        }
    }
}

fn substitute(ty: &InferType, mapping: &HashMap<u32, InferType>) -> InferType {
    match ty {
        InferType::Variable(variable) => mapping
            .get(variable)
            .cloned()
            .unwrap_or(InferType::Variable(*variable)),
        InferType::Application(function, argument) => InferType::Application(
            Box::new(substitute(function, mapping)),
            Box::new(substitute(argument, mapping)),
        ),
        InferType::Function(parameter, result) => InferType::Function(
            Box::new(substitute(parameter, mapping)),
            Box::new(substitute(result, mapping)),
        ),
        InferType::Record(fields) => InferType::Record(
            fields
                .iter()
                .map(|(label, field)| (label.clone(), substitute(field, mapping)))
                .collect(),
        ),
        primitive => primitive.clone(),
    }
}
