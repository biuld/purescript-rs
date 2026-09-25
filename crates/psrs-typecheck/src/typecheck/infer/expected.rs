use super::super::*;

impl Checker {
    /// Uses an annotated function type to check its lambda binders from the
    /// outside in. This makes record shapes in a closed signature available
    /// while inferring field access and update in the function body.
    pub(in crate::typecheck) fn infer_expr_with_expected(
        &mut self,
        expression: &hir::Expr,
        expected: Option<InferType>,
    ) -> Option<InferredExpr> {
        let hir::ExprKind::Lambda { binder, body } = &expression.kind else {
            return self.infer_expr(expression);
        };
        let Some(expected) = expected else {
            return self.infer_expr(expression);
        };
        let expected = self.resolve_type(expected);
        let InferType::Function(parameter, result) = expected else {
            return self.infer_expr(expression);
        };
        let parameter = *parameter;
        let body_expected = Some(*result);
        self.locals
            .insert(binder.id, Scheme::monomorphic(parameter.clone()));
        let body = self.infer_expr_with_expected(body, body_expected);
        self.locals.remove(&binder.id);
        let body = body?;
        let ty = InferType::Function(Box::new(parameter.clone()), Box::new(body.ty.clone()));
        Some(InferredExpr {
            kind: InferredExprKind::Lambda {
                binder: InferredBinder {
                    binder: binder.clone(),
                    scheme: Scheme::monomorphic(parameter),
                },
                body: Box::new(body),
            },
            ty,
            span: expression.span,
        })
    }
}
