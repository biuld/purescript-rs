use super::*;

impl Checker {
    pub(super) fn finalize_expr(
        &mut self,
        expression: InferredExpr,
        interner: &mut TypeInterner,
        generics: &HashSet<u32>,
    ) -> Option<thir::Expr> {
        let ty = self.finalize_type(&expression.ty, expression.span, interner, generics);
        let kind = match expression.kind {
            InferredExprKind::Local(id) => thir::ExprKind::Local(id),
            InferredExprKind::Global(id) => thir::ExprKind::Global(id),
            InferredExprKind::Integer(value) => thir::ExprKind::Integer(value),
            InferredExprKind::Boolean(value) => thir::ExprKind::Boolean(value),
            InferredExprKind::String(value) => thir::ExprKind::String(value),
            InferredExprKind::Application(function, argument) => {
                let function = self.finalize_expr(*function, interner, generics);
                let argument = self.finalize_expr(*argument, interner, generics);
                thir::ExprKind::Application(Box::new(function?), Box::new(argument?))
            }
            InferredExprKind::Lambda { binder, body } => {
                let binder_ty =
                    self.finalize_type(&binder.scheme.ty, binder.binder.span, interner, generics);
                let body = self.finalize_expr(*body, interner, generics);
                thir::ExprKind::Lambda {
                    binder: thir::Binder {
                        id: binder.binder.id,
                        name: binder.binder.name,
                        ty: binder_ty?,
                        span: binder.binder.span,
                    },
                    body: Box::new(body?),
                }
            }
            InferredExprKind::Let { bindings, body } => {
                let bindings = bindings
                    .into_iter()
                    .filter_map(|binding| {
                        let quantified = binding
                            .binder
                            .scheme
                            .variables
                            .iter()
                            .copied()
                            .map(TypeVariableId)
                            .collect();
                        let binder_ty = self.finalize_type(
                            &binding.binder.scheme.ty,
                            binding.binder.binder.span,
                            interner,
                            generics,
                        );
                        let value = self.finalize_expr(binding.value, interner, generics);
                        Some(thir::Binding {
                            binder: thir::Binder {
                                id: binding.binder.binder.id,
                                name: binding.binder.binder.name,
                                ty: binder_ty?,
                                span: binding.binder.binder.span,
                            },
                            quantified,
                            value: value?,
                            span: binding.span,
                        })
                    })
                    .collect();
                thir::ExprKind::Let {
                    bindings,
                    body: Box::new(self.finalize_expr(*body, interner, generics)?),
                }
            }
            InferredExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.finalize_expr(*condition, interner, generics);
                let then_branch = self.finalize_expr(*then_branch, interner, generics);
                let else_branch = self.finalize_expr(*else_branch, interner, generics);
                thir::ExprKind::If {
                    condition: Box::new(condition?),
                    then_branch: Box::new(then_branch?),
                    else_branch: Box::new(else_branch?),
                }
            }
        };
        Some(thir::Expr {
            kind,
            ty: ty?,
            span: expression.span,
        })
    }
}
