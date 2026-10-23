//! Explicit dictionary abstraction for checked and inherited qualified values.

use super::super::*;

impl Checker {
    /// Materializes a value's existing quantifiers and dictionary parameters.
    /// This preserves an inferred polymorphic value, without generalizing any
    /// unknowns introduced by the enclosing local binding.
    pub(in crate::typecheck) fn preserve_quantified_expression(
        &mut self,
        mut expression: InferredExpr,
    ) -> Option<InferredExpr> {
        // Ordinary terms already carry their dictionary function. A pending
        // method is a suspended dictionary field selection, so retaining it as
        // a value requires explicitly binding its dictionary parameters.
        if !matches!(expression.kind, InferredExprKind::PendingMethod { .. }) {
            return Some(expression);
        }
        let ty = self.resolve_type(expression.ty.clone());
        match ty.clone() {
            InferType::ForAll {
                variables, body, ..
            } => {
                expression.ty = *body;
                let mut checked = self.with_skolem_scope(&variables, |checker| {
                    checker.preserve_quantified_expression(expression)
                })?;
                checked.ty = ty;
                Some(checked)
            }
            InferType::Constrained { constraints, body } => {
                let span = expression.span;
                expression.ty = *body.clone();
                self.abstract_constraint_expression(constraints.clone(), *body, span, |checker| {
                    let applied = checker.apply_constraints(expression, constraints, span);
                    checker.preserve_quantified_expression(applied)
                })
            }
            _ => Some(expression),
        }
    }

    /// Each constraint becomes a given while the body is checked or elaborated,
    /// then one dictionary lambda. Both paths share the same scope and evidence
    /// contract, including constraints nested beneath inherited quantifiers.
    pub(super) fn abstract_constraint_expression(
        &mut self,
        constraints: Vec<ClassConstraint>,
        body: InferType,
        span: TextRange,
        elaborate: impl FnOnce(&mut Self) -> Option<InferredExpr>,
    ) -> Option<InferredExpr> {
        let mut binders = Vec::with_capacity(constraints.len());
        for constraint in &constraints {
            let dictionary_type = self.dictionary_type(constraint);
            let id = LocalId(self.state.next_dictionary_local);
            self.state.next_dictionary_local += 1;
            binders.push((id, dictionary_type));
        }
        let givens = constraints
            .iter()
            .cloned()
            .zip(binders.iter().map(|(id, _)| WantedSolution::Given(*id)))
            .collect();
        let mut checked = self.with_givens(givens, elaborate)?;
        for (index, (id, dictionary_type)) in binders.into_iter().enumerate().rev() {
            checked = InferredExpr {
                kind: InferredExprKind::Lambda {
                    binder: InferredBinder {
                        binder: hir::LocalBinder {
                            id,
                            name: "dict".into(),
                            span,
                        },
                        scheme: Scheme::monomorphic(dictionary_type),
                    },
                    body: Box::new(checked),
                },
                ty: InferType::Constrained {
                    constraints: constraints[index..].to_vec(),
                    body: Box::new(body.clone()),
                },
                span,
            };
        }
        Some(checked)
    }
}
