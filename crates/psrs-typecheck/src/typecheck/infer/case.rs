use super::super::*;

impl Checker {
    pub(super) fn infer_case(
        &mut self,
        scrutinee: &hir::Expr,
        branches: &[hir::CaseBranch],
        span: TextRange,
    ) -> Option<InferredExpr> {
        self.infer_case_with_expected(scrutinee, branches, span, None)
    }

    pub(super) fn infer_case_with_expected(
        &mut self,
        scrutinee: &hir::Expr,
        branches: &[hir::CaseBranch],
        span: TextRange,
        expected: Option<InferType>,
    ) -> Option<InferredExpr> {
        let scrutinee = self.infer_expr(scrutinee)?;
        let mut result_ty = expected;
        let mut inferred = Vec::with_capacity(branches.len());
        for branch in branches {
            let mut inserted = Vec::new();
            let pattern = self.check_pattern(&branch.pattern, &scrutinee.ty, &mut inserted)?;
            let value = self.infer_expr_with_expected(&branch.value, result_ty.clone())?;
            for id in inserted {
                self.locals.remove(&id);
            }
            match &result_ty {
                None => result_ty = Some(value.ty.clone()),
                Some(existing) => self.unify(existing.clone(), value.ty.clone(), branch.span),
            }
            inferred.push(InferredCaseBranch {
                pattern,
                value,
                span: branch.span,
            });
        }
        let ty = result_ty.unwrap_or_else(|| self.fresh());
        Some(InferredExpr {
            kind: InferredExprKind::Case {
                scrutinee: Box::new(scrutinee),
                branches: inferred,
            },
            ty,
            span,
        })
    }

    /// Instantiates a constructor's parameter variables and returns its result
    /// type together with its field types.
    pub(super) fn instantiate_constructor(
        &mut self,
        info: &ConstructorInfo,
    ) -> (InferType, Vec<InferType>) {
        let mut variables = HashMap::new();
        let mut arguments = Vec::new();
        for parameter in &info.parameters {
            let variable = self.fresh();
            variables.insert(parameter.clone(), variable.clone());
            arguments.push(variable);
        }
        let mut result = InferType::Constructor(TypeConstructor::User(info.type_id));
        for argument in arguments {
            result = InferType::Application(Box::new(result), Box::new(argument));
        }
        let fields = info
            .fields
            .iter()
            .map(|field| self.elaborate_type(field, &mut variables))
            .collect();
        (result, fields)
    }
}
