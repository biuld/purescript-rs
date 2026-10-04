use super::*;

impl Checker {
    pub(super) fn derive_eq_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        self.derive_structural_eq(method, class_arguments, method.symbol, false, span)
    }

    pub(super) fn derive_eq1_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let Some(eq_method) = self.known_method_symbol("Data.Eq", "Eq", "eq") else {
            return self.deriving_error(span, "cannot find the Eq method for Eq1 deriving");
        };
        self.derive_structural_eq(method, class_arguments, eq_method, true, span)
    }

    fn derive_structural_eq(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        eq_method: SymbolId,
        higher_kinded: bool,
        span: TextRange,
    ) -> Option<InferredExpr> {
        let description = if higher_kinded { "Eq1" } else { "Eq" };
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(span, "equality deriving requires one type argument");
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(
                span,
                &format!("{description} deriving requires a local data or newtype constructor"),
            );
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self
                .deriving_error(span, "cannot find the data declaration to derive equality");
        };
        let fixed_parameters =
            declaration
                .parameters
                .len()
                .checked_sub(if higher_kinded { 1 } else { 0 });
        if type_id.module != self.env.module_id
            || !matches!(
                declaration.kind,
                hir::TypeDeclarationKind::Data | hir::TypeDeclarationKind::Newtype
            )
            || fixed_parameters != Some(arguments.len())
        {
            let requirement = if higher_kinded {
                "a locally declared type constructor with one final parameter"
            } else {
                "a locally declared, fully applied data type"
            };
            return self.deriving_error(
                span,
                &format!("{description} deriving requires {requirement}"),
            );
        }

        let left = self.fresh_deriving_binder("__derived_left", span);
        let right = self.fresh_deriving_binder("__derived_right", span);
        let mut left_case_branches = Vec::new();
        for constructor in &declaration.constructors {
            let left_fields = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__derived_l", span))
                .collect::<Vec<_>>();
            let right_fields = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__derived_r", span))
                .collect::<Vec<_>>();
            let body =
                derive_eq_field_tests(constructor, &left_fields, &right_fields, eq_method, span);
            let same_constructor = hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: constructor_pattern(constructor, &right_fields, span),
                value: body,
                span,
            };
            let mismatch = hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: hir::Pattern {
                    kind: hir::PatternKind::Wildcard,
                    span,
                },
                value: boolean_literal(false, span),
                span,
            };
            let right_case = hir::Expr {
                kind: hir::ExprKind::Case {
                    scrutinee: Box::new(local_expr(right.id, span)),
                    branches: vec![same_constructor, mismatch],
                },
                span,
            };
            left_case_branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: constructor_pattern(constructor, &left_fields, span),
                value: right_case,
                span,
            });
        }
        if left_case_branches.is_empty() {
            left_case_branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: hir::Pattern {
                    kind: hir::PatternKind::Wildcard,
                    span,
                },
                value: boolean_literal(true, span),
                span,
            });
        }
        let implementation = hir::Expr {
            kind: hir::ExprKind::Lambda {
                binder: left.clone(),
                body: Box::new(hir::Expr {
                    kind: hir::ExprKind::Lambda {
                        binder: right.clone(),
                        body: Box::new(hir::Expr {
                            kind: hir::ExprKind::Case {
                                scrutinee: Box::new(local_expr(left.id, span)),
                                branches: left_case_branches,
                            },
                            span,
                        }),
                    },
                    span,
                }),
            },
            span,
        };
        self.infer_derived_method(method, class_arguments, &implementation)
    }
}

fn derive_eq_field_tests(
    constructor: &hir::Constructor,
    left_fields: &[hir::LocalBinder],
    right_fields: &[hir::LocalBinder],
    eq_method: SymbolId,
    span: TextRange,
) -> hir::Expr {
    constructor
        .fields
        .iter()
        .zip(left_fields)
        .zip(right_fields)
        .map(|((_field, left), right)| {
            let method = global_expr(eq_method, span);
            apply_expr(
                apply_expr(method, local_expr(left.id, span), span),
                local_expr(right.id, span),
                span,
            )
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .fold(boolean_literal(true, span), |rest, test| hir::Expr {
            kind: hir::ExprKind::If {
                condition: Box::new(test),
                then_branch: Box::new(rest),
                else_branch: Box::new(boolean_literal(false, span)),
            },
            span,
        })
}

fn constructor_pattern(
    constructor: &hir::Constructor,
    binders: &[hir::LocalBinder],
    span: TextRange,
) -> hir::Pattern {
    hir::Pattern {
        kind: hir::PatternKind::Constructor {
            symbol: constructor.symbol,
            name_span: constructor.name_span,
            arguments: binders
                .iter()
                .cloned()
                .map(|binder| hir::Pattern {
                    kind: hir::PatternKind::Var(binder),
                    span,
                })
                .collect(),
        },
        span,
    }
}
