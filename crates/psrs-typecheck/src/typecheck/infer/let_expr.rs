//! Local `let` and `where` bindings.
//!
//! A binding is generalized before its body so each use instantiates it. The
//! constraints inferred from the binding are part of that scheme when every
//! flexible variable they mention was allocated inside the binding and one
//! binding's type determines them. Other undischarged constraints stay on the
//! enclosing declaration: an outer unknown can still be solved by a later use,
//! and a recursive group does not quantify a variable an open constraint shares.

use super::super::classes::{UnsolvedPolicy, collect_infer_variables};
use super::super::*;
use std::collections::HashSet;

impl Checker {
    pub(in crate::typecheck) fn infer_let_expression(
        &mut self,
        bindings: &[hir::LocalBinding],
        body: &hir::Expr,
        expected: Option<InferType>,
    ) -> Option<(InferredExprKind, InferType)> {
        // Binding bodies are inferred one level deeper, so their unknowns are
        // generalized against this level; the body is checked back at it.
        let outer_level = self.state.level;
        let (inferred_bindings, body) = self.in_nested_level(|checker| {
            let wanted_start = checker.state.wanted.len();
            let mut binders = Vec::with_capacity(bindings.len());
            for binding in bindings {
                let ty = checker.fresh();
                checker
                    .scope
                    .locals
                    .insert(binding.binder.id, Scheme::monomorphic(ty.clone()));
                binders.push(InferredBinder {
                    binder: binding.binder.clone(),
                    scheme: Scheme::monomorphic(ty),
                });
            }
            let mut inferred_bindings = Vec::with_capacity(bindings.len());
            for (binding, binder) in bindings.iter().zip(binders) {
                if let Some(value) = checker.infer_expr(&binding.value) {
                    checker.unify(binder.scheme.ty.clone(), value.ty.clone(), binding.span);
                    inferred_bindings.push(InferredBinding {
                        binder,
                        value,
                        span: binding.span,
                    });
                }
            }
            checker.generalize_let_bindings(
                bindings,
                &mut inferred_bindings,
                outer_level,
                wanted_start,
            );
            checker.state.level = outer_level;
            let body = checker.infer_expr_with_expected(body, expected);
            for binding in bindings {
                checker.scope.locals.remove(&binding.binder.id);
            }
            (inferred_bindings, body)
        });
        let body = body?;
        let ty = body.ty.clone();
        Some((
            InferredExprKind::Let {
                bindings: inferred_bindings,
                body: Box::new(body),
            },
            ty,
        ))
    }

    /// Splits the constraints the bindings raised. Ones determined by a single
    /// binding become its dictionary parameters; the rest keep their unknowns
    /// shared with the enclosing scope.
    fn generalize_let_bindings(
        &mut self,
        bindings: &[hir::LocalBinding],
        inferred: &mut [InferredBinding],
        outer_level: u32,
        wanted_start: usize,
    ) {
        let unsolved = self.solve_wanted_constraints(None, wanted_start, UnsolvedPolicy::Defer);
        let recursive = bindings_are_recursive(bindings);
        let binding_variables = inferred
            .iter()
            .map(|binding| {
                let mut variables = HashSet::new();
                collect_infer_variables(
                    &self.resolve_type(binding.binder.scheme.ty.clone()),
                    &mut variables,
                );
                variables
            })
            .collect::<Vec<_>>();
        let mut owned = vec![Vec::new(); inferred.len()];
        let mut lowered = HashSet::new();
        for index in unsolved {
            let variables = self.constraint_variables(index);
            let deep = self.deep_variables(&variables, outer_level);
            let shares_outer = variables
                .iter()
                .any(|variable| !deep.contains(variable) && !self.state.rigid.contains(variable));
            if deep.is_empty() || recursive || shares_outer {
                lowered.extend(deep);
                continue;
            }
            let owners = binding_variables
                .iter()
                .enumerate()
                .filter(|(_, binding_vars)| {
                    deep.iter().all(|variable| binding_vars.contains(variable))
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if let [owner] = owners.as_slice() {
                let rigid_visible = variables
                    .iter()
                    .filter(|variable| self.state.rigid.contains(variable))
                    .all(|variable| binding_variables[*owner].contains(variable));
                if rigid_visible {
                    owned[*owner].push(index);
                    continue;
                }
            }
            lowered.extend(deep);
        }
        loop {
            let mut changed = false;
            for bucket in &mut owned {
                let mut kept = Vec::new();
                for index in bucket.drain(..) {
                    let deep = self.deep_variables(&self.constraint_variables(index), outer_level);
                    if deep.iter().any(|variable| lowered.contains(variable)) {
                        lowered.extend(deep);
                        changed = true;
                    } else {
                        kept.push(index);
                    }
                }
                *bucket = kept;
            }
            if !changed {
                break;
            }
        }
        for variable in &lowered {
            // Leave the unknown at the enclosing level so this binding does not
            // quantify it while an undischarged constraint still mentions it.
            self.state.levels.insert(*variable, outer_level);
        }
        for (binding, residual) in inferred.iter_mut().zip(owned) {
            let monotype = binding.binder.scheme.ty.clone();
            self.check_residual_ambiguity(
                &self.residual_wanted(&residual),
                &monotype,
                &binding.binder.binder.name,
                binding.span,
            );
            let parameters = self.abstract_dictionaries(&residual);
            let constraints = self.retained_constraints(&residual);
            let mut scheme = self.generalize(&[], &monotype, &constraints, outer_level);
            binding.value = self.wrap_dictionary_lambdas(
                std::mem::replace(
                    &mut binding.value,
                    InferredExpr {
                        kind: InferredExprKind::Integer(0),
                        ty: monotype,
                        span: binding.span,
                    },
                ),
                &parameters,
            );
            scheme = self.generalize_body(scheme, &binding.value, outer_level);
            self.scope
                .locals
                .insert(binding.binder.binder.id, scheme.clone());
            scheme.ty = binding.value.ty.clone();
            binding.binder.scheme = scheme;
        }
    }

    fn constraint_variables(&self, index: usize) -> HashSet<u32> {
        let mut variables = HashSet::new();
        let Some(constraint) = self.state.wanted.get(index) else {
            return variables;
        };
        for argument in &constraint.arguments {
            collect_infer_variables(&self.resolve_type(argument.clone()), &mut variables);
        }
        variables
    }

    fn deep_variables(&self, variables: &HashSet<u32>, outer_level: u32) -> HashSet<u32> {
        variables
            .iter()
            .copied()
            .filter(|variable| {
                !self.state.rigid.contains(variable)
                    && self
                        .state
                        .levels
                        .get(variable)
                        .copied()
                        .unwrap_or(TOP_LEVEL)
                        > outer_level
            })
            .collect()
    }
}

fn bindings_are_recursive(bindings: &[hir::LocalBinding]) -> bool {
    let ids = bindings
        .iter()
        .map(|binding| binding.binder.id)
        .collect::<HashSet<_>>();
    bindings
        .iter()
        .any(|binding| expr_mentions(&binding.value, &ids))
}

fn expr_mentions(expression: &hir::Expr, ids: &HashSet<hir::LocalId>) -> bool {
    match &expression.kind {
        hir::ExprKind::Local(id) => ids.contains(id),
        hir::ExprKind::Application(function, argument) => {
            expr_mentions(function, ids) || expr_mentions(argument, ids)
        }
        hir::ExprKind::Operator { left, right, .. } => {
            expr_mentions(left, ids) || expr_mentions(right, ids)
        }
        hir::ExprKind::Negate {
            function,
            expression,
            ..
        } => expr_mentions(function, ids) || expr_mentions(expression, ids),
        hir::ExprKind::Typed { expression, .. }
        | hir::ExprKind::TypeApplication { expression, .. }
        | hir::ExprKind::FieldAccess { expression, .. } => expr_mentions(expression, ids),
        hir::ExprKind::Array(elements) => {
            elements.iter().any(|element| expr_mentions(element, ids))
        }
        hir::ExprKind::Record(fields) | hir::ExprKind::MatchProduct(fields) => {
            fields.iter().any(|(_, value)| expr_mentions(value, ids))
        }
        hir::ExprKind::RecordUpdate { expression, fields } => {
            expr_mentions(expression, ids)
                || fields.iter().any(|(_, value)| expr_mentions(value, ids))
        }
        hir::ExprKind::OperatorChain { operands, .. } => {
            operands.iter().any(|operand| expr_mentions(operand, ids))
        }
        hir::ExprKind::OperatorSection { operand, .. } => expr_mentions(operand, ids),
        hir::ExprKind::Lambda { body, .. } => expr_mentions(body, ids),
        hir::ExprKind::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| expr_mentions(&binding.value, ids))
                || expr_mentions(body, ids)
        }
        hir::ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_mentions(condition, ids)
                || expr_mentions(then_branch, ids)
                || expr_mentions(else_branch, ids)
        }
        hir::ExprKind::Case {
            scrutinee,
            branches,
        } => {
            expr_mentions(scrutinee, ids)
                || branches
                    .iter()
                    .any(|branch| expr_mentions(&branch.value, ids))
        }
        hir::ExprKind::Guarded(clauses) => clauses.iter().any(|clause| {
            expr_mentions(&clause.value, ids)
                || clause.guards.iter().any(|guard| guard_mentions(guard, ids))
                || clause
                    .where_bindings
                    .iter()
                    .any(|binding| expr_mentions(&binding.value, ids))
        }),
        hir::ExprKind::Global(_)
        | hir::ExprKind::Integer(_)
        | hir::ExprKind::Number(_)
        | hir::ExprKind::String(_)
        | hir::ExprKind::Char(_) => false,
    }
}

fn guard_mentions(guard: &hir::Guard, ids: &HashSet<hir::LocalId>) -> bool {
    match guard {
        hir::Guard::Boolean(expression) => expr_mentions(expression, ids),
        hir::Guard::Pattern { value, .. } => expr_mentions(value, ids),
        hir::Guard::Let { bindings, .. } => bindings
            .iter()
            .any(|binding| expr_mentions(&binding.value, ids)),
    }
}
