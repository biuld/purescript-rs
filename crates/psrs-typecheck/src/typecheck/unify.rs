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
            | (InferType::Boolean, InferType::Boolean)
            | (InferType::String, InferType::String)
            | (InferType::Unit, InferType::Unit) => {}
            (InferType::Function(a1, r1), InferType::Function(a2, r2)) => {
                self.unify(*a1, *a2, span);
                self.unify(*r1, *r2, span);
            }
            (expected, actual) => self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TypeMismatch,
                span,
                format!("type mismatch: expected {expected}, found {actual}"),
            )),
        }
    }

    fn bind_variable(&mut self, variable: u32, ty: InferType, span: TextRange) {
        if occurs(variable, &ty) {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::OccursCheck,
                span,
                format!("infinite type: _T{variable} occurs in {ty}"),
            ));
        } else {
            let level = self.levels.get(&variable).copied().unwrap_or(TOP_LEVEL);
            self.adjust_levels(&ty, level);
            self.substitutions.insert(variable, ty);
        }
    }

    fn signature_mismatch(&mut self, expected: InferType, found: InferType, span: TextRange) {
        self.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::TypeMismatch,
            span,
            format!("signature mismatch: expected {expected}, found {found}"),
        ));
    }

    pub(super) fn resolve_type(&self, ty: InferType) -> InferType {
        match ty {
            InferType::Variable(variable) => self
                .substitutions
                .get(&variable)
                .map(|ty| self.resolve_type(ty.clone()))
                .unwrap_or(InferType::Variable(variable)),
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
            InferType::Function(parameter, result) => {
                self.adjust_levels(parameter, max_level);
                self.adjust_levels(result, max_level);
            }
            InferType::I32 | InferType::Boolean | InferType::String | InferType::Unit => {}
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
            InferType::Function(parameter, result) => {
                self.collect_generalizable(parameter, outer_level, out);
                self.collect_generalizable(result, outer_level, out);
            }
            InferType::I32 | InferType::Boolean | InferType::String | InferType::Unit => {}
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
            InferType::Boolean => Some(interner.intern(Type::Boolean)),
            InferType::String => Some(interner.intern(Type::String)),
            InferType::Unit => Some(interner.intern(Type::Unit)),
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
        InferType::Function(parameter, result) => InferType::Function(
            Box::new(substitute(parameter, mapping)),
            Box::new(substitute(result, mapping)),
        ),
        primitive => primitive.clone(),
    }
}
