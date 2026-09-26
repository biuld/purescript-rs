use super::super::*;

impl Checker {
    pub(super) fn infer_record(
        &mut self,
        fields: &[(String, hir::Expr)],
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        let mut inferred = Vec::with_capacity(fields.len());
        let mut labels = HashSet::new();
        for (label, value) in fields {
            if !labels.insert(label) {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::TypeMismatch,
                    span,
                    format!("record label `{label}` occurs more than once"),
                ));
                return None;
            }
            inferred.push((label.clone(), self.infer_expr(value)?));
        }
        let mut record_fields = inferred
            .iter()
            .map(|(label, value)| (label.clone(), value.ty.clone()))
            .collect::<Vec<_>>();
        record_fields.sort_by(|left, right| left.0.cmp(&right.0));
        let ty = InferType::Record(InferRecord::closed(record_fields));
        Some((InferredExprKind::Record(inferred), ty))
    }

    pub(super) fn infer_field_access(
        &mut self,
        expression: &hir::Expr,
        field: &str,
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        let expression = self.infer_expr(expression)?;
        let field_ty = self.fresh();
        let tail = match self.fresh() {
            InferType::Variable(variable) => variable,
            _ => unreachable!("fresh inference types are variables"),
        };
        // `{ field :: a | r }`. A closed record solves `r`; a missing label
        // cannot extend a closed or rigid tail.
        self.unify(
            expression.ty.clone(),
            InferType::Record(InferRecord {
                fields: vec![(field.to_owned(), field_ty.clone())],
                tail: RowTail::Open(tail),
            }),
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
        let mut inferred = Vec::with_capacity(fields.len());
        let mut probed = Vec::with_capacity(fields.len());
        let mut labels = HashSet::new();
        for (label, value) in fields {
            if !labels.insert(label) {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::TypeMismatch,
                    span,
                    format!("record label `{label}` occurs more than once"),
                ));
                continue;
            }
            let value = self.infer_expr(value)?;
            probed.push((label.clone(), self.fresh()));
            inferred.push((label.clone(), value));
        }
        probed.sort_by(|left, right| left.0.cmp(&right.0));
        let tail = match self.fresh() {
            InferType::Variable(variable) => variable,
            _ => unreachable!("fresh inference types are variables"),
        };
        // The original record must contain the updated labels. Their new types
        // replace those labels and share the un-updated tail. `Prim.Row.Cons`
        // would be required to update a label that is only in an unknown tail.
        self.unify(
            expression.ty.clone(),
            InferType::Record(InferRecord {
                fields: probed,
                tail: RowTail::Open(tail),
            }),
            span,
        );
        let mut result_fields = inferred
            .iter()
            .map(|(label, value)| (label.clone(), value.ty.clone()))
            .collect::<Vec<_>>();
        result_fields.sort_by(|left, right| left.0.cmp(&right.0));
        Some((
            InferredExprKind::RecordUpdate {
                expression: Box::new(expression),
                fields: inferred,
            },
            InferType::Record(InferRecord {
                fields: result_fields,
                tail: RowTail::Open(tail),
            }),
        ))
    }
}
