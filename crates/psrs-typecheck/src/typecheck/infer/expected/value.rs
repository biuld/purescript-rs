//! Adapt a checked value without reinterpreting or erasing its lexical body.

use super::super::super::*;

impl Checker {
    /// A compound polymorphic value needs its definition's quantifier scope
    /// retained while a reference is instantiated. Atomic references already
    /// have that boundary at their declaration. Pending methods retain their
    /// existing dictionary-selection path rather than becoming stored values.
    pub(in crate::typecheck) fn prepare_expression_use(
        &mut self,
        expression: InferredExpr,
    ) -> (Option<InferredBinding>, InferredExpr) {
        if matches!(
            self.resolve_type(expression.ty.clone()),
            InferType::ForAll { .. } | InferType::Constrained { .. }
        ) && !matches!(
            expression.kind,
            InferredExprKind::Local(_)
                | InferredExprKind::Global(_)
                | InferredExprKind::PendingMethod { .. }
        ) {
            let (binding, reference) = self.bind_inferred_expression(expression);
            (Some(binding), reference)
        } else {
            (None, expression)
        }
    }

    fn bind_inferred_expression(
        &mut self,
        expression: InferredExpr,
    ) -> (InferredBinding, InferredExpr) {
        let span = expression.span;
        let binder = hir::LocalBinder {
            id: LocalId(self.state.next_dictionary_local),
            name: "__checked_value".into(),
            span,
        };
        self.state.next_dictionary_local += 1;
        let scheme = Scheme::monomorphic(expression.ty.clone());
        let reference = InferredExpr {
            kind: InferredExprKind::Local(binder.id),
            ty: self.synthesize_scheme_type(&scheme),
            span,
        };
        (
            InferredBinding {
                binder: InferredBinder { binder, scheme },
                value: expression,
                span,
            },
            reference,
        )
    }

    pub(in crate::typecheck) fn finish_expression_use(
        &mut self,
        binding: Option<InferredBinding>,
        body: InferredExpr,
    ) -> InferredExpr {
        let Some(binding) = binding else {
            return body;
        };
        InferredExpr {
            // The result type owns a separate, alpha-equivalent binder scope.
            // Reusing the body's binders here would nest them inside themselves.
            ty: self.freshen_foralls(&self.resolve_type(body.ty.clone())),
            span: binding.span,
            kind: InferredExprKind::Let {
                bindings: vec![binding],
                body: Box::new(body),
            },
        }
    }

    /// Ordinary local checking owns expected givens, subsumption and evidence.
    /// The bound value retains its original nested types and evaluates once.
    pub(in crate::typecheck) fn check_inferred_expression(
        &mut self,
        expression: InferredExpr,
        expected: InferType,
    ) -> Option<InferredExpr> {
        let (binding, reference) = self.bind_inferred_expression(expression);
        let usage = hir::Expr {
            kind: hir::ExprKind::Local(binding.binder.binder.id),
            span: reference.span,
        };
        let body = self.with_scope(|checker| {
            checker
                .scope
                .locals
                .insert(binding.binder.binder.id, binding.binder.scheme.clone());
            checker.infer_expr_with_expected(&usage, Some(expected))
        })?;
        Some(self.finish_expression_use(Some(binding), body))
    }
}
