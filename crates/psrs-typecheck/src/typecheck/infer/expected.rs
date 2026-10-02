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

        if let hir::ExprKind::Typed {
            expression: inner,
            ty,
        } = &expression.kind
        {
            let saved_substitutions = self.substitutions.clone();
            let saved_levels = self.levels.clone();
            let saved_generics = self.generic_variables.clone();
            let saved_rigid = self.rigid.clone();
            let saved_locals = self.locals.clone();
            let saved_givens = self.givens.clone();
            let saved_given_rigid = self.given_rigid.clone();
            let saved_wanted = self.wanted.clone();
            let saved_reported_fundeps = self.reported_fundep_conflicts.clone();
            let saved_annotation_variables = self.annotation_variables.clone();
            let saved_type_variable_names = self.type_variable_names.clone();
            let saved_infer_variable_kinds = self.infer_variable_kinds.clone();
            let saved_level = self.level;
            let errors_before = self.errors.len();

            let mut annotation_variables = self.annotation_variables.clone();
            let annotation = self.elaborate_type(ty, &mut annotation_variables);
            let checked = self.infer_expr_with_expected(inner, Some(annotation));
            let valid = checked.is_some() && self.errors.len() == errors_before;

            self.substitutions = saved_substitutions;
            self.levels = saved_levels;
            self.generic_variables = saved_generics;
            self.rigid = saved_rigid;
            self.locals = saved_locals;
            self.givens = saved_givens;
            self.given_rigid = saved_given_rigid;
            self.wanted = saved_wanted;
            self.reported_fundep_conflicts = saved_reported_fundeps;
            self.annotation_variables = saved_annotation_variables;
            self.type_variable_names = saved_type_variable_names;
            self.infer_variable_kinds = saved_infer_variable_kinds;
            self.level = saved_level;

            if !valid {
                return None;
            }

            let errors_before = self.errors.len();
            let mut annotation_variables = self.annotation_variables.clone();
            let annotation = self.elaborate_type(ty, &mut annotation_variables);
            self.subsume(annotation, expected.clone(), expression.span);
            if self.errors.len() != errors_before {
                return None;
            }
            return self.infer_expr_with_expected(inner, Some(self.resolve_type(expected)));
        }

        if let InferType::ForAll { variables, body } = expected.clone() {
            let outer_level = self.level;
            let skolem_level = outer_level + 1;
            self.level = skolem_level;
            let previous_annotation_variables = self.annotation_variables.clone();
            for variable in &variables {
                if let Some(name) = self.type_variable_names.get(variable) {
                    self.annotation_variables
                        .insert(name.clone(), InferType::Variable(*variable));
                }
            }
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
            self.annotation_variables = previous_annotation_variables;
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
        let mut variables = self.annotation_variables.clone();
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
