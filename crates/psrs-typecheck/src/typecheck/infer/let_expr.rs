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
                // An annotated local is predeclared at its written type so a
                // recursive use instantiates the annotation instead of a fresh
                // metavariable that a first recursive use would bind to an
                // arrow.
                let scheme = checker.predeclared_local_scheme(binding);
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

    /// The scheme a local binder is predeclared with. An explicit annotation is
    /// elaborated here; an elaboration that reports a diagnostic is discarded so
    /// the ascription reports it once. An unannotated local is monomorphic.
    fn predeclared_local_scheme(&mut self, binding: &hir::LocalBinding) -> Scheme {
        let Some(ty) = local_annotation(binding) else {
            return Scheme::monomorphic(self.fresh());
        };
        let mut variables = self.scope.annotation_variables.clone();
        let annotation = self.speculate(|checker| {
            let errors_before = checker.state.errors.len();
            let annotation = checker.elaborate_type(ty, &mut variables);
            (checker.state.errors.len() == errors_before).then_some(annotation)
        });
        match annotation {
            Some(annotation) => Scheme::monomorphic(annotation),
            None => Scheme::monomorphic(self.fresh()),
        }
    }
}

/// The written type of an annotated local binding, which the resolver stores as
/// a `Typed` wrapper on the binding value.
fn local_annotation(binding: &hir::LocalBinding) -> Option<&hir::Type> {
    match &binding.value.kind {
        hir::ExprKind::Typed { ty, .. } => Some(ty),
        _ => None,
    }
}
