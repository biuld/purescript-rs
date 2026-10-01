use super::super::*;

impl Checker {
    pub(super) fn infer_application(
        &mut self,
        function: &hir::Expr,
        argument: &hir::Expr,
        span: TextRange,
    ) -> Option<(InferredExprKind, InferType)> {
        let mut function = self.infer_expr(function)?;
        let (constraints, instantiated) =
            self.instantiate_use(&Scheme::monomorphic(function.ty.clone()));
        function.ty = instantiated;
        function = self.apply_constraints(function, constraints, span);
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
        let argument = self.infer_expr_with_expected(argument, Some(parameter.clone()))?;
        let result = self.resolve_type(result);
        Some((
            InferredExprKind::Application(Box::new(function), Box::new(argument)),
            result,
        ))
    }
}
