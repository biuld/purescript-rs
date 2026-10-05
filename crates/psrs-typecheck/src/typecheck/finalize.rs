use super::*;

impl Checker {
    pub(super) fn finalize_expr(
        &mut self,
        expression: InferredExpr,
        interner: &mut TypeInterner,
        generics: &HashSet<u32>,
    ) -> Option<thir::Expr> {
        let mut active_generics = generics.clone();
        let mut scope = self.resolve_type(expression.ty.clone());
        while let InferType::ForAll { variables, body } = scope {
            active_generics.extend(variables);
            scope = self.resolve_type(*body);
        }
        let generics = &active_generics;
        let ty = self.finalize_type(&expression.ty, expression.span, interner, generics);
        let kind = match expression.kind {
            InferredExprKind::Local(id) => thir::ExprKind::Local(id),
            InferredExprKind::Global(id) => thir::ExprKind::Global(id),
            InferredExprKind::Integer(value) => thir::ExprKind::Integer(value),
            InferredExprKind::Number(value) => thir::ExprKind::Number(value),
            InferredExprKind::Boolean(value) => thir::ExprKind::Boolean(value),
            InferredExprKind::String(value) => thir::ExprKind::String(value),
            InferredExprKind::Char(value) => thir::ExprKind::Char(value),
            InferredExprKind::Array(elements) => thir::ExprKind::Array(
                elements
                    .into_iter()
                    .map(|element| self.finalize_expr(element, interner, generics))
                    .collect::<Option<Vec<_>>>()?,
            ),
            InferredExprKind::Record(fields) => thir::ExprKind::Record(
                fields
                    .into_iter()
                    .map(|(label, value)| {
                        Some((label, self.finalize_expr(value, interner, generics)?))
                    })
                    .collect::<Option<Vec<_>>>()?,
            ),
            InferredExprKind::RecordUpdate { expression, fields } => thir::ExprKind::RecordUpdate {
                expression: Box::new(self.finalize_expr(*expression, interner, generics)?),
                fields: fields
                    .into_iter()
                    .map(|(label, value)| {
                        Some((label, self.finalize_expr(value, interner, generics)?))
                    })
                    .collect::<Option<Vec<_>>>()?,
            },
            InferredExprKind::FieldAccess { expression, field } => thir::ExprKind::FieldAccess {
                expression: Box::new(self.finalize_expr(*expression, interner, generics)?),
                field,
            },
            InferredExprKind::Method { method, wanted } => {
                let evidence = self.wanted_evidence(wanted, interner, generics)?;
                let dictionary_ty = evidence.ty;
                let evidence = thir::Expr {
                    kind: thir::ExprKind::Evidence(evidence),
                    ty: dictionary_ty,
                    span: expression.span,
                };
                thir::ExprKind::FieldAccess {
                    expression: Box::new(evidence),
                    field: method,
                }
            }
            InferredExprKind::DictionaryApplication { function, wanted } => {
                let function = self.finalize_expr(*function, interner, generics)?;
                let evidence = self.wanted_evidence(wanted, interner, generics)?;
                let dictionary_ty = evidence.ty;
                let evidence = thir::Expr {
                    kind: thir::ExprKind::Evidence(evidence),
                    ty: dictionary_ty,
                    span: expression.span,
                };
                thir::ExprKind::Application(Box::new(function), Box::new(evidence))
            }
            InferredExprKind::CoerceFunction {
                wanted,
                source,
                target,
            } => {
                let source_type =
                    self.finalize_type(&source, expression.span, interner, generics)?;
                let target_type =
                    self.finalize_type(&target, expression.span, interner, generics)?;
                let evidence = self.wanted_evidence(wanted, interner, generics)?;
                let local = LocalId(self.state.next_dictionary_local);
                self.state.next_dictionary_local += 1;
                let binder = thir::Binder {
                    id: local,
                    name: "__coerce_value".to_owned(),
                    ty: source_type,
                    span: expression.span,
                };
                let value = thir::Expr {
                    kind: thir::ExprKind::Local(local),
                    ty: source_type,
                    span: expression.span,
                };
                let body = thir::Expr {
                    kind: thir::ExprKind::Coerce {
                        value: Box::new(value),
                        evidence,
                        source_type,
                        target_type,
                    },
                    ty: target_type,
                    span: expression.span,
                };
                thir::ExprKind::Lambda {
                    binder,
                    body: Box::new(body),
                }
            }
            InferredExprKind::UnsafeCoerceFunction { source, target } => {
                let source_type =
                    self.finalize_type(&source, expression.span, interner, generics)?;
                let target_type =
                    self.finalize_type(&target, expression.span, interner, generics)?;
                let local = LocalId(self.state.next_dictionary_local);
                self.state.next_dictionary_local += 1;
                let binder = thir::Binder {
                    id: local,
                    name: "__unsafe_coerce_value".to_owned(),
                    ty: source_type,
                    span: expression.span,
                };
                let value = thir::Expr {
                    kind: thir::ExprKind::Local(local),
                    ty: source_type,
                    span: expression.span,
                };
                let body = thir::Expr {
                    kind: thir::ExprKind::UnsafeCoerce {
                        value: Box::new(value),
                        source_type,
                        target_type,
                    },
                    ty: target_type,
                    span: expression.span,
                };
                thir::ExprKind::Lambda {
                    binder,
                    body: Box::new(body),
                }
            }
            InferredExprKind::Evidence(wanted) => {
                let evidence = self.wanted_evidence(wanted, interner, generics)?;
                thir::ExprKind::Evidence(evidence)
            }
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
            InferredExprKind::Case {
                scrutinee,
                branches,
            } => {
                let scrutinee = self.finalize_expr(*scrutinee, interner, generics)?;
                let branches = branches
                    .into_iter()
                    .map(|branch| {
                        let pattern = self.finalize_pattern(branch.pattern, interner, generics)?;
                        let value = self.finalize_expr(branch.value, interner, generics)?;
                        Some(thir::CaseBranch {
                            pattern,
                            value,
                            span: branch.span,
                            coverage: branch.coverage,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                thir::ExprKind::Case {
                    scrutinee: Box::new(scrutinee),
                    branches,
                }
            }
        };
        Some(thir::Expr {
            kind,
            ty: ty?,
            span: expression.span,
        })
    }

    fn finalize_pattern(
        &mut self,
        pattern: InferredPattern,
        interner: &mut TypeInterner,
        generics: &HashSet<u32>,
    ) -> Option<thir::Pattern> {
        let ty = self.finalize_type(&pattern.ty, pattern.span, interner, generics)?;
        let kind = match pattern.kind {
            InferredPatternKind::Wildcard => thir::PatternKind::Wildcard,
            InferredPatternKind::Literal { literal } => thir::PatternKind::Literal { literal },
            InferredPatternKind::Array { elements } => thir::PatternKind::Array {
                elements: elements
                    .into_iter()
                    .map(|element| self.finalize_pattern(element, interner, generics))
                    .collect::<Option<Vec<_>>>()?,
            },
            InferredPatternKind::Named { binder, pattern } => thir::PatternKind::Named {
                id: binder.id,
                pattern: Box::new(self.finalize_pattern(*pattern, interner, generics)?),
            },
            InferredPatternKind::Var { binder, ty } => {
                let ty = self.finalize_type(&ty, binder.span, interner, generics)?;
                thir::PatternKind::Var { id: binder.id, ty }
            }
            InferredPatternKind::Constructor { symbol, arguments } => {
                let arguments = arguments
                    .into_iter()
                    .map(|argument| self.finalize_pattern(argument, interner, generics))
                    .collect::<Option<Vec<_>>>()?;
                thir::PatternKind::Constructor { symbol, arguments }
            }
            InferredPatternKind::Record { fields } => thir::PatternKind::Record {
                fields: fields
                    .into_iter()
                    .map(|(label, pattern)| {
                        Some((label, self.finalize_pattern(pattern, interner, generics)?))
                    })
                    .collect::<Option<Vec<_>>>()?,
            },
        };
        Some(thir::Pattern {
            kind,
            ty,
            span: pattern.span,
        })
    }
}
