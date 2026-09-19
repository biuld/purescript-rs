use super::super::*;

impl Checker {
    /// Checks a pattern against the scrutinee type and binds its variables.
    pub(super) fn check_pattern(
        &mut self,
        pattern: &hir::Pattern,
        expected: &InferType,
        inserted: &mut Vec<LocalId>,
    ) -> Option<InferredPattern> {
        let span = pattern.span;
        let kind = match &pattern.kind {
            hir::PatternKind::Wildcard => InferredPatternKind::Wildcard,
            hir::PatternKind::Var(binder) => {
                self.locals
                    .insert(binder.id, Scheme::monomorphic(expected.clone()));
                inserted.push(binder.id);
                InferredPatternKind::Var {
                    binder: binder.clone(),
                    ty: expected.clone(),
                }
            }
            hir::PatternKind::Constructor {
                symbol, arguments, ..
            } => {
                let Some(info) = self.constructor_info.get(symbol).cloned() else {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::InvalidHir,
                        span,
                        "pattern constructor has no type declaration",
                    ));
                    return None;
                };
                let (result, fields) = self.instantiate_constructor(&info);
                self.unify(expected.clone(), result, span);
                if arguments.len() != fields.len() {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::TypeMismatch,
                        span,
                        format!(
                            "constructor `{}` expects {} arguments but the pattern has {}",
                            info.name,
                            fields.len(),
                            arguments.len()
                        ),
                    ));
                    return None;
                }
                let mut lowered = Vec::with_capacity(arguments.len());
                for (argument, field) in arguments.iter().zip(fields) {
                    lowered.push(self.check_pattern(argument, &field, inserted)?);
                }
                InferredPatternKind::Constructor {
                    symbol: *symbol,
                    arguments: lowered,
                }
            }
            hir::PatternKind::Record { fields } => {
                let expected = self.resolve_type(expected.clone());
                let record_fields = match &expected {
                    InferType::Record(fields) => fields.clone(),
                    InferType::Variable(_) => {
                        let mut fields = fields
                            .iter()
                            .map(|(label, _)| (label.clone(), self.fresh()))
                            .collect::<Vec<_>>();
                        fields.sort_by(|left, right| left.0.cmp(&right.0));
                        self.unify(expected.clone(), InferType::Record(fields.clone()), span);
                        fields
                    }
                    _ => {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::UnsupportedExpression,
                            span,
                            "record pattern requires a concrete record type",
                        ));
                        return None;
                    }
                };
                let mut labels = HashSet::new();
                let mut lowered = Vec::with_capacity(fields.len());
                for (label, field_pattern) in fields {
                    if !labels.insert(label) {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::TypeMismatch,
                            span,
                            format!("record label `{label}` occurs more than once"),
                        ));
                        return None;
                    }
                    let Some((_, field_ty)) = record_fields
                        .iter()
                        .find(|(field_label, _)| field_label == label)
                    else {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::TypeMismatch,
                            span,
                            format!("record has no field `{label}`"),
                        ));
                        return None;
                    };
                    lowered.push((
                        label.clone(),
                        self.check_pattern(field_pattern, field_ty, inserted)?,
                    ));
                }
                InferredPatternKind::Record { fields: lowered }
            }
        };
        Some(InferredPattern {
            kind,
            ty: self.resolve_type(expected.clone()),
            span,
        })
    }
}
