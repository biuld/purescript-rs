use super::super::*;

impl Checker {
    pub(super) fn infer_record_with_expected(
        &mut self,
        fields: &[(String, hir::Expr)],
        span: TextRange,
        expected: &InferType,
    ) -> Option<(InferredExprKind, InferType)> {
        let expected_row = record_row(expected)?;
        let FlatRow {
            fields: expected_fields,
            ..
        } = self.normalize_row_or_report(expected_row, span);
        let expected_fields = expected_fields.into_iter().collect::<HashMap<_, _>>();
        self.infer_record_fields(fields, span, |checker, label, value| match expected_fields
            .get(label)
        {
            Some(expected) => checker.infer_expr_with_expected(value, Some(expected.clone())),
            None => checker.infer_record_field(value),
        })
    }

    pub(super) fn infer_record(
        &mut self,
        fields: &[(String, hir::Expr)],
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        self.infer_record_fields(fields, span, |checker, _, value| {
            checker.infer_record_field(value)
        })
    }

    /// Official `inferWithinRecord` instantiates names and their visible type
    /// applications. An explicitly typed field keeps its polymorphic boundary.
    fn infer_record_field(&mut self, value: &hir::Expr) -> Option<InferredExpr> {
        let inferred = self.infer_expr(value)?;
        Some(if field_should_instantiate(value) {
            self.instantiate_all_expr(inferred, value.span)
        } else {
            inferred
        })
    }

    pub(super) fn infer_record_fields(
        &mut self,
        fields: &[(String, hir::Expr)],
        span: TextRange,
        mut infer_field: impl FnMut(&mut Self, &str, &hir::Expr) -> Option<InferredExpr>,
    ) -> Option<(InferredExprKind, InferType)> {
        let mut inferred = Vec::with_capacity(fields.len());
        let mut labels = HashSet::new();
        for (label, value) in fields {
            if !labels.insert(label) {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::TypeMismatch,
                    span,
                    format!("record label `{label}` occurs more than once"),
                ));
                return None;
            }
            inferred.push((label.clone(), infer_field(self, label, value)?));
        }
        let ty = record_type(
            inferred
                .iter()
                .map(|(label, value)| (label.clone(), value.ty.clone()))
                .collect(),
            InferType::RowEmpty,
        );
        Some((InferredExprKind::Record(inferred), ty))
    }

    pub(super) fn infer_field_access(
        &mut self,
        expression: &hir::Expr,
        field: &str,
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        let expression = self.infer_expr(expression)?;
        let expression_span = expression.span;
        let expression = self.instantiate_all_expr(expression, expression_span);
        let field_ty = self.fresh();
        let tail = match self.fresh() {
            InferType::Variable(variable) => variable,
            _ => unreachable!("fresh inference types are variables"),
        };
        // `{ field :: a | r }`. A closed record solves `r`; a missing label
        // cannot extend a closed or rigid tail.
        self.unify(
            expression.ty.clone(),
            record_type(
                vec![(field.to_owned(), field_ty.clone())],
                InferType::Variable(tail),
            ),
            span,
        );
        Some((
            InferredExprKind::FieldAccess {
                expression: Box::new(expression),
                field: field.to_owned(),
            },
            field_ty,
        ))
    }

    pub(super) fn infer_record_update(
        &mut self,
        expression: &hir::Expr,
        fields: &[(String, hir::Expr)],
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        let expression = self.infer_expr(expression)?;
        let expression_span = expression.span;
        let expression = self.instantiate_all_expr(expression, expression_span);
        let mut inferred = Vec::with_capacity(fields.len());
        let mut probed = Vec::with_capacity(fields.len());
        let mut labels = HashSet::new();
        for (label, value) in fields {
            if !labels.insert(label) {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::TypeMismatch,
                    span,
                    format!("record label `{label}` occurs more than once"),
                ));
                continue;
            }
            let value = self.infer_record_field(value)?;
            probed.push((label.clone(), self.fresh()));
            inferred.push((label.clone(), value));
        }
        let tail = match self.fresh() {
            InferType::Variable(variable) => variable,
            _ => unreachable!("fresh inference types are variables"),
        };
        // The original record must contain the updated labels. Their new types
        // replace those labels and share the un-updated tail. `Prim.Row.Cons`
        // would be required to update a label that is only in an unknown tail.
        self.unify(
            expression.ty.clone(),
            record_type(probed, InferType::Variable(tail)),
            span,
        );
        let result_fields = inferred
            .iter()
            .map(|(label, value)| (label.clone(), value.ty.clone()))
            .collect::<Vec<_>>();
        Some((
            InferredExprKind::RecordUpdate {
                expression: Box::new(expression),
                fields: inferred,
            },
            record_type(result_fields, InferType::Variable(tail)),
        ))
    }
}

fn field_should_instantiate(expression: &hir::Expr) -> bool {
    match &expression.kind {
        hir::ExprKind::Local(_) | hir::ExprKind::Global(_) => true,
        hir::ExprKind::TypeApplication { expression, .. } => field_should_instantiate(expression),
        _ => false,
    }
}
