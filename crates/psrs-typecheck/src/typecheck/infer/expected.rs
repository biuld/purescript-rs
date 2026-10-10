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
        psrs_span::with_sufficient_stack(|| {
            self.infer_expr_with_expected_inner(expression, expected)
        })
    }

    fn infer_expr_with_expected_inner(
        &mut self,
        expression: &hir::Expr,
        expected: Option<InferType>,
    ) -> Option<InferredExpr> {
        let Some(expected) = expected else {
            return self.infer_expr(expression);
        };
        let expected = self.resolve_type(expected);

        if let hir::ExprKind::Typed {
            expression: inner,
            ty,
        } = &expression.kind
        {
            let mut checked = self.speculate_reporting(|checker| {
                let errors_before = checker.state.errors.len();
                let mut annotation_variables = checker.scope.annotation_variables.clone();
                let annotation = checker.elaborate_type(ty, &mut annotation_variables);
                checker.probe_reporting(|trial| {
                    let errors_before = trial.state.errors.len();
                    trial.infer_expr_with_expected(inner, Some(annotation.clone()))?;
                    (trial.state.errors.len() == errors_before).then_some(())
                })?;

                let polymorphic_expected = matches!(&expected, InferType::ForAll { .. });
                let used = if polymorphic_expected {
                    checker.subsume(annotation.clone(), expected.clone(), expression.span);
                    checker.infer_expr_with_expected(inner, Some(expected.clone()))?
                } else {
                    let (constraints, body) =
                        checker.instantiate_use(&Scheme::monomorphic(annotation));
                    checker.subsume(body.clone(), expected.clone(), expression.span);
                    let constraints = constraints
                        .into_iter()
                        .map(|mut constraint| {
                            constraint.arguments = constraint
                                .arguments
                                .iter()
                                .map(|argument| checker.resolve_type(argument.clone()))
                                .collect();
                            constraint
                        })
                        .collect::<Vec<_>>();
                    let body = checker.resolve_type(body);
                    let use_type = if constraints.is_empty() {
                        body.clone()
                    } else {
                        InferType::Constrained {
                            constraints: constraints.clone(),
                            body: Box::new(body.clone()),
                        }
                    };
                    let inferred = checker.infer_expr_with_expected(inner, Some(use_type))?;
                    let base = InferredExpr {
                        kind: inferred.kind,
                        ty: body,
                        span: expression.span,
                    };
                    checker.apply_constraints(base, constraints, expression.span)
                };
                (checker.state.errors.len() == errors_before).then_some(used)
            })?;
            checked.ty = self.resolve_type(expected);
            return Some(checked);
        }

        if let InferType::ForAll { variables, body } = expected.clone() {
            let checked = self.with_scope(|checker| {
                for variable in &variables {
                    if let Some(name) = checker.scope.type_variable_names.get(variable) {
                        checker
                            .scope
                            .annotation_variables
                            .insert(name.clone(), InferType::Variable(*variable));
                    }
                }
                checker.with_skolem_scope(&variables, |checker| {
                    checker.infer_expr_with_expected(expression, Some(*body.clone()))
                })
            });
            let mut checked = checked?;
            checked.ty = expected;
            return Some(checked);
        }

        if let InferType::Constrained { constraints, body } = expected.clone() {
            // Each constraint becomes a given, and its argument variables are
            // rigid for as long as the given is in scope.
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
            let checked = self.with_givens(givens, |checker| {
                checker.infer_expr_with_expected(expression, Some(*body.clone()))
            });
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
            self.scope
                .locals
                .insert(binder.id, Scheme::monomorphic(parameter.clone()));
            let body = self.infer_expr_with_expected(body, Some((**result).clone()));
            self.scope.locals.remove(&binder.id);
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

        let inferred = self.infer_expr(expression)?;
        // A bare class-method use keeps its method-local quantifiers and
        // constraints on the inferred type. Instantiating them here matches the
        // application path, so `eq = eq1` provides the `Eq a` dictionary the
        // method signature requires instead of unifying the constraint with a
        // plain function type.
        let scheme = Scheme::monomorphic(inferred.ty.clone());
        let mut inferred = self.instantiate_expression_use(inferred, &scheme, expression.span);
        if matches!(
            inferred.kind,
            InferredExprKind::CoerceFunction { .. } | InferredExprKind::UnsafeCoerceFunction { .. }
        ) {
            // A cast's source and target are exact boundary types, rather than
            // a function implementation adapted by contravariant subsumption.
            // In particular, keep a contextual rank-N input quantified in the
            // source metadata that finalization emits as the cast's binder.
            self.unify(expected.clone(), inferred.ty.clone(), expression.span);
        } else {
            self.subsume(inferred.ty.clone(), expected.clone(), expression.span);
        }
        inferred.ty = expected;
        Some(inferred)
    }
}

impl Checker {
    /// Checks `e :: T`: the written type is elaborated with rigid variables,
    /// exactly as a signature is, and the expression is checked against it.
    ///
    /// The ascription produces no node of its own. It is a type-directed check
    /// at a known expression, so the result is the expression carrying the
    /// ascription's type. A leading `forall` is handled by
    /// [`Self::infer_expr_with_expected`], which checks the body against skolems
    /// and returns the quantifier, so `((\_ -> 0) :: forall b. b -> Int)` is
    /// checked at that polymorphic type and keeps it.
    pub(in crate::typecheck) fn infer_ascription(
        &mut self,
        expression: &hir::Expr,
        ty: &hir::Type,
        span: TextRange,
    ) -> Option<InferredExpr> {
        let mut variables = self.scope.annotation_variables.clone();
        let expected = self.elaborate_type(ty, &mut variables);
        let mut checked = self.infer_expr_with_expected(expression, Some(expected.clone()))?;
        checked.ty = expected;
        Some(InferredExpr {
            kind: checked.kind,
            ty: checked.ty,
            span,
        })
    }
}
