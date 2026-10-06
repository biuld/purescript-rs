use super::super::*;

impl Checker {
    pub(super) fn infer_case(
        &mut self,
        scrutinee: &hir::Expr,
        branches: &[hir::CaseBranch],
        span: TextRange,
    ) -> Option<InferredExpr> {
        self.infer_case_with_expected(scrutinee, branches, span, None)
    }

    pub(super) fn infer_case_with_expected(
        &mut self,
        scrutinee: &hir::Expr,
        branches: &[hir::CaseBranch],
        span: TextRange,
        expected: Option<InferType>,
    ) -> Option<InferredExpr> {
        let requires_monotype = branches
            .iter()
            .any(|branch| pattern_requires_monotype(&branch.pattern));
        let scrutinee = if let hir::ExprKind::MatchProduct(fields) = &scrutinee.kind {
            let (kind, ty) =
                self.infer_record_fields(fields, scrutinee.span, |checker, label, value| {
                    let requires_monotype = branches.iter().any(|branch| {
                        product_field_pattern(&branch.pattern, label)
                            .is_none_or(pattern_requires_monotype)
                    });
                    checker.infer_pattern_scrutinee(value, requires_monotype)
                })?;
            InferredExpr {
                kind,
                ty,
                span: scrutinee.span,
            }
        } else {
            self.infer_pattern_scrutinee(scrutinee, requires_monotype)?
        };
        let mut result_ty = expected;
        let mut inferred = Vec::with_capacity(branches.len());
        for branch in branches {
            if branch.coverage == hir::CaseBranchCoverage::PartialFallback {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::InvalidHir,
                    branch.span,
                    "partial fallback authorization must be issued by type checking",
                ));
                return None;
            }
            let mut inserted = Vec::new();
            let pattern = self.check_pattern(&branch.pattern, &scrutinee.ty, &mut inserted)?;
            let value = self.infer_expr_with_expected(&branch.value, result_ty.clone())?;
            for id in inserted {
                self.scope.locals.remove(&id);
            }
            match &result_ty {
                None => result_ty = Some(value.ty.clone()),
                Some(existing) => self.unify(existing.clone(), value.ty.clone(), branch.span),
            }
            inferred.push(InferredCaseBranch {
                pattern,
                value,
                span: branch.span,
                coverage: branch.coverage,
            });
        }
        let ty = result_ty.unwrap_or_else(|| self.fresh());
        // Report-only Partial has no runtime dictionary, but its checked
        // authorization must survive erasure. Reuse the generated empty-case
        // failure representation used by guard desugaring; the marked fallback
        // contributes coverage and never hides redundant source alternatives.
        if !branches.is_empty()
            && self.scope.givens.iter().any(|(constraint, _)| {
                constraint.class_id == hir::TypeId::PRIM_PARTIAL && constraint.arguments.is_empty()
            })
        {
            inferred.push(InferredCaseBranch {
                pattern: InferredPattern {
                    kind: InferredPatternKind::Wildcard,
                    ty: scrutinee.ty.clone(),
                    span,
                },
                value: InferredExpr {
                    kind: InferredExprKind::Case {
                        scrutinee: Box::new(InferredExpr {
                            kind: InferredExprKind::Integer(0),
                            ty: InferType::Constructor(TypeConstructor::Int),
                            span,
                        }),
                        branches: Vec::new(),
                    },
                    ty: ty.clone(),
                    span,
                },
                span,
                coverage: hir::CaseBranchCoverage::PartialFallback,
            });
        }
        Some(InferredExpr {
            kind: InferredExprKind::Case {
                scrutinee: Box::new(scrutinee),
                branches: inferred,
            },
            ty,
            span,
        })
    }

    /// A local pattern-matching scrutinee retains structural `forall` types so
    /// a typed binder can check a rank-N pattern against the value's declared
    /// type. Ordinary local references still instantiate those quantifiers.
    pub(super) fn infer_pattern_scrutinee(
        &mut self,
        expression: &hir::Expr,
        requires_monotype: bool,
    ) -> Option<InferredExpr> {
        if requires_monotype {
            return self.infer_expr(expression);
        }
        let hir::ExprKind::Local(id) = expression.kind else {
            return self.infer_expr(expression);
        };
        let Some(scheme) = self.scope.locals.get(&id).cloned() else {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::InvalidHir,
                expression.span,
                "local pattern scrutinee has no type environment entry",
            ));
            return None;
        };
        let mapping = self
            .instantiate_type_variables(scheme.variables.iter().copied(), &scheme.variable_kinds);
        let ty = super::super::unify::substitute(&scheme.ty, &mapping);
        let ty = self.freshen_foralls(&ty);
        let ty = self.resolve_type(ty);
        let constraints = scheme
            .constraints
            .iter()
            .map(|constraint| ClassConstraint {
                arguments: constraint
                    .arguments
                    .iter()
                    .map(|argument| super::super::unify::substitute(argument, &mapping))
                    .collect(),
                ..constraint.clone()
            })
            .collect();
        let value = InferredExpr {
            kind: InferredExprKind::Local(id),
            ty,
            span: expression.span,
        };
        Some(self.apply_constraints(value, constraints, expression.span))
    }

    /// Instantiates a constructor's parameter variables and returns its result
    /// type together with its field types.
    pub(super) fn instantiate_constructor(
        &mut self,
        info: &ConstructorInfo,
    ) -> (InferType, Vec<InferType>) {
        let mut variables = HashMap::new();
        let mut arguments = Vec::new();
        for parameter in &info.parameters {
            let variable = self.fresh();
            variables.insert(parameter.clone(), variable.clone());
            arguments.push(variable);
        }
        let mut result = InferType::Constructor(TypeConstructor::User(info.type_id));
        for argument in arguments {
            result = InferType::Application(Box::new(result), Box::new(argument));
        }
        let fields = info
            .fields
            .iter()
            .map(|field| self.elaborate_type(field, &mut variables))
            .collect();
        (result, fields)
    }
}

fn product_field_pattern<'a>(pattern: &'a hir::Pattern, label: &str) -> Option<&'a hir::Pattern> {
    match &pattern.kind {
        hir::PatternKind::Record { fields, .. } => fields
            .iter()
            .find(|(field, _)| field == label)
            .map(|(_, pattern)| pattern),
        hir::PatternKind::Named { pattern, .. } => product_field_pattern(pattern, label),
        _ => None,
    }
}

fn pattern_requires_monotype(pattern: &hir::Pattern) -> bool {
    match &pattern.kind {
        hir::PatternKind::Wildcard | hir::PatternKind::Var(_) => false,
        hir::PatternKind::Named { pattern, .. } => pattern_requires_monotype(pattern),
        hir::PatternKind::Typed { pattern, ty } => {
            type_requires_monotype(ty) || pattern_requires_monotype(pattern)
        }
        hir::PatternKind::OperatorChain { .. } => true,
        hir::PatternKind::Constructor { .. }
        | hir::PatternKind::Record { .. }
        | hir::PatternKind::Boolean(_)
        | hir::PatternKind::Integer(_)
        | hir::PatternKind::Number(_)
        | hir::PatternKind::String(_)
        | hir::PatternKind::Char(_)
        | hir::PatternKind::Array(_) => true,
    }
}

fn type_requires_monotype(ty: &hir::Type) -> bool {
    // PureScript's `isMonoType` distinguishes only a leading forall. Nested
    // forall types inside an arrow remain part of an otherwise monomorphic
    // scrutinee type.
    !matches!(ty.kind, hir::TypeKind::Forall { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty(kind: hir::TypeKind, start: u32, end: u32) -> hir::Type {
        hir::Type {
            kind,
            span: TextRange::new(start, end),
        }
    }

    #[test]
    fn only_a_leading_forall_is_a_polytype_for_pattern_instantiation() {
        let variable = ty(hir::TypeKind::Variable("a".into()), 1, 2);
        let function = || {
            ty(
                hir::TypeKind::Function {
                    parameter: Box::new(variable.clone()),
                    result: Box::new(variable.clone()),
                },
                1,
                4,
            )
        };
        let quantified = ty(
            hir::TypeKind::Forall {
                variables: vec![hir::TypeParameter {
                    name: "a".into(),
                    name_span: TextRange::new(0, 1),
                    kind: None,
                }],
                body: Box::new(function()),
            },
            0,
            4,
        );
        let nested = ty(
            hir::TypeKind::Function {
                parameter: Box::new(quantified),
                result: Box::new(function()),
            },
            0,
            8,
        );

        assert!(!type_requires_monotype(&ty(
            hir::TypeKind::Forall {
                variables: vec![hir::TypeParameter {
                    name: "a".into(),
                    name_span: TextRange::new(0, 1),
                    kind: None,
                }],
                body: Box::new(function()),
            },
            0,
            4,
        )));
        assert!(type_requires_monotype(&nested));
    }
}
