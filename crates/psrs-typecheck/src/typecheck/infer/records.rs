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
        inferred.sort_by(|left, right| left.0.cmp(&right.0));
        let ty = InferType::Record(
            inferred
                .iter()
                .map(|(label, value)| (label.clone(), value.ty.clone()))
                .collect(),
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
}
