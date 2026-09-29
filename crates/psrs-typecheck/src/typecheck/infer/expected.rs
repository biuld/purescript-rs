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
        // A trusted effect-library definition writes the runtime representation
        // directly: a lambda over the hidden context parameter. Checking it
        // against the opaque `Effect a` keeps source `Effect a` nominal while
        // still allowing the representation to be authored. The binder is an
        // integer context value, never a token type in Core.
        if self.effect_runtime_representation
            && let InferType::Application(function, argument) = &expected
            && matches!(**function, InferType::Constructor(TypeConstructor::Effect))
        {
            let argument = (**argument).clone();
            let binder_ty = InferType::I32;
            self.locals
                .insert(binder.id, Scheme::monomorphic(binder_ty.clone()));
            let body = self.infer_expr_with_expected(body, Some(argument.clone()));
            self.locals.remove(&binder.id);
            let body = body?;
            return Some(InferredExpr {
                kind: InferredExprKind::Lambda {
                    binder: InferredBinder {
                        binder: binder.clone(),
                        scheme: Scheme::monomorphic(binder_ty),
                    },
                    body: Box::new(body),
                },
                ty: InferType::Application(
                    Box::new(InferType::Constructor(TypeConstructor::Effect)),
                    Box::new(argument),
                ),
                span: expression.span,
            });
        }
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
