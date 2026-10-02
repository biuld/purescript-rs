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
            hir::PatternKind::Boolean(value) => {
                self.check_literal_type(expected, TypeConstructor::Boolean, span);
                InferredPatternKind::Literal {
                    literal: thir::PatternLiteral::Boolean(*value),
                }
            }
            hir::PatternKind::Integer(text) => {
                let Ok(value) = text.parse::<i32>() else {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::IntegerOutOfRange,
                        span,
                        format!("integer literal `{text}` is outside the signed 32-bit range"),
                    ));
                    return None;
                };
                self.check_literal_type(expected, TypeConstructor::Int, span);
                InferredPatternKind::Literal {
                    literal: thir::PatternLiteral::Integer(value),
                }
            }
            hir::PatternKind::Number(text) => {
                if !text.parse::<f64>().is_ok_and(f64::is_finite) {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::NumberOutOfRange,
                        span,
                        "number literal is not a valid Number",
                    ));
                    return None;
                }
                self.check_literal_type(expected, TypeConstructor::Number, span);
                InferredPatternKind::Literal {
                    literal: thir::PatternLiteral::Number(text.clone()),
                }
            }
            hir::PatternKind::String(value) => {
                self.check_literal_type(expected, TypeConstructor::String, span);
                InferredPatternKind::Literal {
                    literal: thir::PatternLiteral::String(value.clone()),
                }
            }
            hir::PatternKind::Char(value) => {
                self.check_literal_type(expected, TypeConstructor::Char, span);
                InferredPatternKind::Literal {
                    literal: thir::PatternLiteral::Char(*value),
                }
            }
            hir::PatternKind::Array(elements) => {
                let element_ty = self.fresh();
                let array_ty = InferType::Application(
                    Box::new(InferType::Constructor(TypeConstructor::Array)),
                    Box::new(element_ty.clone()),
                );
                self.unify(expected.clone(), array_ty, span);
                let elements = elements
                    .iter()
                    .map(|element| self.check_pattern(element, &element_ty, inserted))
                    .collect::<Option<Vec<_>>>()?;
                InferredPatternKind::Array { elements }
            }
            hir::PatternKind::Var(binder) => {
                self.bind_pattern_local(binder, expected, inserted);
                InferredPatternKind::Var {
                    binder: binder.clone(),
                    ty: expected.clone(),
                }
            }
            hir::PatternKind::Named { binder, pattern } => {
                self.bind_pattern_local(binder, expected, inserted);
                InferredPatternKind::Named {
                    binder: binder.clone(),
                    pattern: Box::new(self.check_pattern(pattern, expected, inserted)?),
                }
            }
            hir::PatternKind::Typed { pattern, ty } => {
                let annotation = self.elaborate_type(ty, &mut self.annotation_variables.clone());
                self.unify(expected.clone(), annotation.clone(), ty.span);
                return self.check_pattern(pattern, &annotation, inserted);
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
            hir::PatternKind::OperatorChain { .. } => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnloweredOperator,
                    span,
                    "operator patterns must be resolved before type checking",
                ));
                return None;
            }
            hir::PatternKind::Record { fields, mode } => {
                let expected = self.resolve_type(expected.clone());
                let mut record_fields = if let Some(row) = record_row(&expected) {
                    let normalized = self.normalize_row_or_report(row, span);
                    (normalized.fields, normalized.tail)
                } else if matches!(expected, InferType::Variable(_)) {
                    let inferred_fields = fields
                        .iter()
                        .map(|(label, _)| (label.clone(), self.fresh()))
                        .collect::<Vec<_>>();
                    let tail = match mode {
                        hir::RecordPatternMode::Partial => RowTail::Open(self.fresh_row()),
                        hir::RecordPatternMode::Exact => RowTail::Closed,
                    };
                    self.unify(
                        expected.clone(),
                        record_type(inferred_fields.clone(), tail.to_type()),
                        span,
                    );
                    (inferred_fields, tail)
                } else {
                    self.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::UnsupportedExpression,
                        span,
                        "record pattern requires a record type",
                    ));
                    return None;
                };
                if *mode == hir::RecordPatternMode::Exact {
                    let requested_labels = fields
                        .iter()
                        .map(|(label, _)| label.as_str())
                        .collect::<HashSet<_>>();
                    let extra_labels = record_fields
                        .0
                        .iter()
                        .map(|(label, _)| label.as_str())
                        .filter(|label| !requested_labels.contains(label))
                        .collect::<Vec<_>>();
                    if !extra_labels.is_empty() {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::TypeMismatch,
                            span,
                            format!(
                                "exact record pattern omits fields {}",
                                extra_labels.join(", ")
                            ),
                        ));
                        return None;
                    }
                }
                let mut labels = HashSet::new();
                let mut lowered = Vec::with_capacity(fields.len());
                for (label, field_pattern) in fields {
                    if !labels.insert(label) {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::TypeMismatch,
                            field_pattern.span,
                            format!("record label `{label}` occurs more than once"),
                        ));
                        return None;
                    }
                    let field_ty = if let Some((_, field_ty)) = record_fields
                        .0
                        .iter()
                        .find(|(field_label, _)| field_label == label)
                    {
                        field_ty.clone()
                    } else if let RowTail::Open(tail) = record_fields.1 {
                        let field_ty = self.fresh();
                        let remaining_tail = self.fresh_row();
                        self.unify(
                            InferType::Variable(tail),
                            row_from_fields(
                                vec![(label.clone(), field_ty.clone())],
                                InferType::Variable(remaining_tail),
                            ),
                            field_pattern.span,
                        );
                        record_fields.1 = RowTail::Open(remaining_tail);
                        record_fields.0.push((label.clone(), field_ty.clone()));
                        field_ty
                    } else {
                        self.errors.push(TypeCheckError::new(
                            TypeCheckErrorKind::TypeMismatch,
                            field_pattern.span,
                            format!("record has no field `{label}`"),
                        ));
                        return None;
                    };
                    lowered.push((
                        label.clone(),
                        self.check_pattern(field_pattern, &field_ty, inserted)?,
                    ));
                }
                if *mode == hir::RecordPatternMode::Exact
                    && let RowTail::Open(tail) = record_fields.1
                {
                    self.unify(InferType::Variable(tail), InferType::RowEmpty, span);
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

    fn bind_pattern_local(
        &mut self,
        binder: &LocalBinder,
        ty: &InferType,
        inserted: &mut Vec<LocalId>,
    ) {
        self.locals
            .insert(binder.id, Scheme::monomorphic(ty.clone()));
        inserted.push(binder.id);
    }

    fn check_literal_type(
        &mut self,
        expected: &InferType,
        primitive: TypeConstructor,
        span: TextRange,
    ) {
        self.unify(expected.clone(), InferType::Constructor(primitive), span);
    }
}
