use super::super::*;

impl Checker {
    /// Checks an expression against an expected type. Leading quantifiers are
    /// checked rigidly; lambdas, records, and arrays receive their expected
    /// component types before their children are inferred.
    pub(in crate::typecheck) fn infer_expr_with_expected(
        &mut self,
        expression: &hir::Expr,
        expected: Option<InferType>,
    ) -> Option<InferredExpr> {
        let Some(expected) = expected else {
            return self.infer_expr(expression);
        };
        let expected = self.resolve_type(expected);

        if let InferType::ForAll { variables, body } = expected.clone() {
            let outer_level = self.level;
            let skolem_level = outer_level + 1;
            self.level = skolem_level;
            let previous_levels = variables
                .iter()
                .map(|variable| {
                    let previous = self.levels.insert(*variable, skolem_level);
                    self.rigid.insert(*variable);
                    (*variable, previous)
                })
                .collect::<Vec<_>>();
            let checked = self.infer_expr_with_expected(expression, Some(*body));
            self.level = outer_level;
            for (variable, previous) in previous_levels {
                if let Some(previous) = previous {
                    self.levels.insert(variable, previous);
                }
            }
            let mut checked = checked?;
            checked.ty = expected;
            return Some(checked);
        }

        if let InferType::Constrained { constraints, body } = expected.clone() {
            let previous_givens = self.givens.clone();
            let previous_rigid = self.rigid.clone();
            let previous_given_rigid = self.given_rigid.clone();
            let mut binders = Vec::with_capacity(constraints.len());
            for constraint in &constraints {
                let dictionary_type = self.dictionary_type(constraint);
                let id = LocalId(self.next_dictionary_local);
                self.next_dictionary_local += 1;
                for argument in &constraint.arguments {
                    let mut variables = HashSet::new();
                    super::super::classes::collect_infer_variables(argument, &mut variables);
                    for variable in variables {
                        if self.rigid.insert(variable) {
                            self.given_rigid.push(variable);
                        }
                    }
                }
                self.givens
                    .push((constraint.clone(), WantedSolution::Given(id)));
                binders.push((id, dictionary_type));
            }
            let checked = self.infer_expr_with_expected(expression, Some(*body.clone()));
            self.givens = previous_givens;
            self.rigid = previous_rigid;
            self.given_rigid = previous_given_rigid;
            let mut checked = checked?;
            for (index, (id, dictionary_type)) in binders.into_iter().enumerate().rev() {
                let ty = InferType::Constrained {
                    constraints: constraints[index..].to_vec(),
                    body: body.clone(),
                };
                checked = InferredExpr {
                    kind: InferredExprKind::Lambda {
                        binder: InferredBinder {
                            binder: hir::LocalBinder {
                                id,
                                name: "dict".into(),
                                span: expression.span,
                            },
                            scheme: Scheme::monomorphic(dictionary_type),
                        },
                        body: Box::new(checked),
                    },
                    ty,
                    span: expression.span,
                };
            }
            checked.ty = expected;
            return Some(checked);
        }

        if let hir::ExprKind::Lambda { binder, body } = &expression.kind
            && let InferType::Application(inner, result) = &expected
            && let Some((parameter, _)) = infer_arrow_parts(inner, result)
        {
            self.locals
                .insert(binder.id, Scheme::monomorphic(parameter.clone()));
            let body = self.infer_expr_with_expected(body, Some((**result).clone()));
            self.locals.remove(&binder.id);
            let body = body?;
            let actual = arrow(parameter.clone(), body.ty.clone());
            self.subsume(actual, expected.clone(), expression.span);
            return Some(InferredExpr {
                kind: InferredExprKind::Lambda {
                    binder: InferredBinder {
                        binder: binder.clone(),
                        scheme: Scheme::monomorphic(parameter),
                    },
                    body: Box::new(body),
                },
                ty: expected,
                span: expression.span,
            });
        }

        if let hir::ExprKind::Record(fields) = &expression.kind
            && record_row(&expected).is_some()
        {
            let (kind, actual) =
                self.infer_record_with_expected(fields, expression.span, &expected)?;
            self.subsume(actual, expected.clone(), expression.span);
            return Some(InferredExpr {
                kind,
                ty: expected,
                span: expression.span,
            });
        }

        if let hir::ExprKind::Array(elements) = &expression.kind
            && let InferType::Application(function, element) = &expected
            && matches!(**function, InferType::Constructor(TypeConstructor::Array))
        {
            let elements = elements
                .iter()
                .map(|element_expr| {
                    self.infer_expr_with_expected(element_expr, Some((**element).clone()))
                })
                .collect::<Option<Vec<_>>>()?;
            return Some(InferredExpr {
                kind: InferredExprKind::Array(elements),
                ty: expected,
                span: expression.span,
            });
        }

        if let hir::ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } = &expression.kind
        {
            let condition = self.infer_expr_with_expected(
                condition,
                Some(InferType::Constructor(TypeConstructor::Boolean)),
            )?;
            let then_branch = self.infer_expr_with_expected(then_branch, Some(expected.clone()))?;
            let else_branch = self.infer_expr_with_expected(else_branch, Some(expected.clone()))?;
            return Some(InferredExpr {
                kind: InferredExprKind::If {
                    condition: Box::new(condition),
                    then_branch: Box::new(then_branch),
                    else_branch: Box::new(else_branch),
                },
                ty: expected,
                span: expression.span,
            });
        }

        if let hir::ExprKind::Let { bindings, body } = &expression.kind {
            let (kind, ty) = self.infer_let_expression(bindings, body, Some(expected.clone()))?;
            self.subsume(ty, expected.clone(), expression.span);
            return Some(InferredExpr {
                kind,
                ty: expected,
                span: expression.span,
            });
        }

        if let hir::ExprKind::Case {
            scrutinee,
            branches,
        } = &expression.kind
        {
            return self.infer_case_with_expected(
                scrutinee,
                branches,
                expression.span,
                Some(expected),
            );
        }

        let mut inferred = self.infer_expr(expression)?;
        self.subsume(inferred.ty.clone(), expected.clone(), expression.span);
        inferred.ty = expected;
        Some(inferred)
    }
}
