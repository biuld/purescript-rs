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
        let ty = InferType::Record(record_fields);
        Some((InferredExprKind::Record(inferred), ty))
    }

    pub(super) fn infer_field_access(
        &mut self,
        expression: &hir::Expr,
        field: &str,
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        let expression = self.infer_expr(expression)?;
        let record_ty = self.resolve_type(expression.ty.clone());
        let InferType::Record(fields) = record_ty else {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedExpression,
                span,
                "field access requires a concrete record type",
            ));
            return None;
        };
        let Some((_, field_ty)) = fields.iter().find(|(label, _)| label == field) else {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TypeMismatch,
                span,
                format!("record has no field `{field}`"),
            ));
            return None;
        };
        Some((
            InferredExprKind::FieldAccess {
                expression: Box::new(expression),
                field: field.to_owned(),
            },
            field_ty.clone(),
        ))
    }

    pub(super) fn infer_record_update(
        &mut self,
        expression: &hir::Expr,
        fields: &[(String, hir::Expr)],
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        let expression = self.infer_expr(expression)?;
        let record_ty = self.resolve_type(expression.ty.clone());
        let InferType::Record(record_fields) = record_ty.clone() else {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedExpression,
                span,
                "record update requires a concrete record type",
            ));
            return None;
        };

        let mut inferred = Vec::with_capacity(fields.len());
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
            let Some((_, field_ty)) = record_fields.iter().find(|(name, _)| name == label) else {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::TypeMismatch,
                    span,
                    format!("record has no field `{label}`"),
                ));
                continue;
            };
            let value = self.infer_expr(value)?;
            self.unify(field_ty.clone(), value.ty.clone(), value.span);
            inferred.push((label.clone(), value));
        }
        Some((
            InferredExprKind::RecordUpdate {
                expression: Box::new(expression),
                fields: inferred,
            },
            record_ty,
        ))
    }
}
