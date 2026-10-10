//! Local `let` and `where` bindings share the enclosing declaration's unknowns.
//!
//! Unannotated locals are monomorphic, including their class obligations.
//! Solving and generalization belong to the enclosing declaration. An explicit
//! polymorphic annotation retains its checked quantifiers and is instantiated
//! independently at each local use.

use super::super::*;

impl Checker {
    pub(in crate::typecheck) fn infer_let_expression(
        &mut self,
        bindings: &[hir::LocalBinding],
        body: &hir::Expr,
        expected: Option<InferType>,
    ) -> Option<(InferredExprKind, InferType)> {
        let previous_locals = self.scope.locals.clone();
        let result = self.with_scope(|checker| {
            let mut binders = Vec::with_capacity(bindings.len());
            for binding in bindings {
                let scheme = Scheme::monomorphic(checker.fresh());
                checker
                    .scope
                    .locals
                    .insert(binding.binder.id, scheme.clone());
                binders.push(InferredBinder {
                    binder: binding.binder.clone(),
                    scheme,
                });
            }
            let mut inferred = Vec::with_capacity(bindings.len());
            for (binding, mut binder) in bindings.iter().zip(binders) {
                let value = checker.infer_expr(&binding.value)?;
                checker.unify(binder.scheme.ty, value.ty.clone(), binding.span);
                binder.scheme = Scheme::monomorphic(value.ty.clone());
                checker
                    .scope
                    .locals
                    .insert(binding.binder.id, binder.scheme.clone());
                inferred.push(InferredBinding {
                    binder,
                    value,
                    span: binding.span,
                });
            }
            let body = checker.infer_expr_with_expected(body, expected)?;
            let ty = body.ty.clone();
            Some((
                InferredExprKind::Let {
                    bindings: inferred,
                    body: Box::new(body),
                },
                ty,
            ))
        });
        self.scope.locals = previous_locals;
        result
    }
}
