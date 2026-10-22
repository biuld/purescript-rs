use super::super::*;

impl Checker {
    pub(super) fn infer_application(
        &mut self,
        function: &hir::Expr,
        argument: &hir::Expr,
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        let application = self.infer_application_with_expected(function, argument, span, None)?;
        Some((application.kind, application.ty))
    }

    pub(super) fn infer_application_with_expected(
        &mut self,
        function: &hir::Expr,
        argument: &hir::Expr,
        span: TextRange,
        expected: Option<InferType>,
    ) -> Option<InferredExpr> {
        let function = self.infer_expr(function)?;
        // Value application instantiates every leading quantifier, visible or
        // not, the way `checkFunctionApplication` does. A visible application
        // is the only form that stops at a visible binder.
        let function = self.instantiate_all_expr(function, span);
        let function_type = self.resolve_type(function.ty.clone());
        let (parameter, result) = if let InferType::Application(inner, result) =
            function_type.clone()
            && let Some((parameter, result)) = infer_arrow_parts(&inner, &result)
        {
            (parameter, result)
        } else {
            let parameter = self.fresh();
            let result = self.fresh();
            self.unify(
                function_type,
                arrow(parameter.clone(), result.clone()),
                span,
            );
            (parameter, result)
        };
        // A contextual result constrains the callable before an argument opens
        // a nested skolem scope. Otherwise a result already known to be Int,
        // for example, is treated as an unconstrained outer unknown and a
        // rigid mismatch is prematurely diagnosed as a skolem escape.
        // Use the ordinary scheme operation, retaining result dictionaries
        // until the actual application exists and can receive their evidence.
        let result_use = expected.as_ref().map(|expected| {
            let (constraints, body) = self.instantiate_use(&Scheme::monomorphic(result.clone()));
            self.subsume(body.clone(), expected.clone(), span);
            (constraints, body)
        });
        let argument = self.infer_expr_with_expected(argument, Some(parameter))?;
        let application = InferredExpr {
            kind: InferredExprKind::Application(Box::new(function), Box::new(argument)),
            ty: self.resolve_type(result),
            span,
        };
        match (expected, result_use) {
            (Some(expected), Some((constraints, body))) => {
                let application = InferredExpr {
                    ty: self.resolve_type(body),
                    ..application
                };
                let mut application = self.apply_constraints(application, constraints, span);
                application.ty = self.resolve_type(expected);
                Some(application)
            }
            _ => Some(application),
        }
    }
}
