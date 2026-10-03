use super::super::super::*;
use super::flatten_spine;

#[derive(Clone, Copy)]
struct OrdFieldContext {
    method: SymbolId,
    less: SymbolId,
    equal: SymbolId,
    greater: SymbolId,
    span: TextRange,
}

impl Checker {
    pub(super) fn derive_ord_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(span, "Ord deriving requires one type argument");
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(
                span,
                "Ord deriving requires a local data or newtype constructor",
            );
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(span, "cannot find the data declaration to derive Ord");
        };
        if type_id.module != self.env.module_id
            || !matches!(
                declaration.kind,
                hir::TypeDeclarationKind::Data | hir::TypeDeclarationKind::Newtype
            )
            || arguments.len() != declaration.parameters.len()
        {
            return self.deriving_error(
                span,
                "Ord deriving requires a locally declared, fully applied data type",
            );
        }
        let Some(ordering_id) = ordering_result_id(&method.signature) else {
            return self.deriving_error(
                span,
                "Ord deriving requires a method returning the Ordering data type",
            );
        };
        let Some(ordering) = self.env.type_declarations.get(&ordering_id) else {
            return self.deriving_error(span, "cannot find the Ordering data declaration");
        };
        let Some(less) = nullary_constructor(ordering, "LT") else {
            return self.deriving_error(span, "Ordering must define a nullary LT constructor");
        };
        let Some(equal) = nullary_constructor(ordering, "EQ") else {
            return self.deriving_error(span, "Ordering must define a nullary EQ constructor");
        };
        let Some(greater) = nullary_constructor(ordering, "GT") else {
            return self.deriving_error(span, "Ordering must define a nullary GT constructor");
        };

        let left = self.fresh_deriving_binder("__derived_left", span);
        let right = self.fresh_deriving_binder("__derived_right", span);
        let field_context = OrdFieldContext {
            method: method.symbol,
            less,
            equal,
            greater,
            span,
        };
        let mut left_branches = Vec::new();
        for (left_index, constructor) in declaration.constructors.iter().enumerate() {
            let left_fields = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__derived_l", span))
                .collect::<Vec<_>>();
            let mut right_branches = Vec::new();
            for (right_index, other) in declaration.constructors.iter().enumerate() {
                let result_symbol = match left_index.cmp(&right_index) {
                    std::cmp::Ordering::Less => less,
                    std::cmp::Ordering::Equal => equal,
                    std::cmp::Ordering::Greater => greater,
                };
                let right_fields = other
                    .fields
                    .iter()
                    .map(|_| self.fresh_deriving_binder("__derived_r", span))
                    .collect::<Vec<_>>();
                let value = if left_index == right_index {
                    derive_ord_field_tests(
                        &constructor.fields,
                        &left_fields,
                        &right_fields,
                        field_context,
                    )
                } else {
                    global_expr(result_symbol, span)
                };
                right_branches.push(hir::CaseBranch {
                    coverage: hir::CaseBranchCoverage::Source,
                    pattern: constructor_pattern(other.symbol, &right_fields, span),
                    value,
                    span,
                });
            }
            let right_case = hir::Expr {
                kind: hir::ExprKind::Case {
                    scrutinee: Box::new(local_expr(right.id, span)),
                    branches: right_branches,
                },
                span,
            };
            left_branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: constructor_pattern(constructor.symbol, &left_fields, span),
                value: right_case,
                span,
            });
        }
        if left_branches.is_empty() {
            left_branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: hir::Pattern {
                    kind: hir::PatternKind::Wildcard,
                    span,
                },
                value: global_expr(equal, span),
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
                                branches: left_branches,
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

fn derive_ord_field_tests(
    fields: &[hir::Type],
    left: &[hir::LocalBinder],
    right: &[hir::LocalBinder],
    context: OrdFieldContext,
) -> hir::Expr {
    let OrdFieldContext {
        method,
        less,
        equal,
        greater,
        span,
    } = context;
    fields.iter().zip(left).zip(right).rev().fold(
        global_expr(equal, span),
        |rest, ((_field, left), right)| {
            let compared = apply_expr(
                apply_expr(global_expr(method, span), local_expr(left.id, span), span),
                local_expr(right.id, span),
                span,
            );
            hir::Expr {
                kind: hir::ExprKind::Case {
                    scrutinee: Box::new(compared),
                    branches: vec![
                        ordering_branch(less, global_expr(less, span), span),
                        ordering_branch(equal, rest, span),
                        ordering_branch(greater, global_expr(greater, span), span),
                    ],
                },
                span,
            }
        },
    )
}

fn ordering_branch(symbol: SymbolId, value: hir::Expr, span: TextRange) -> hir::CaseBranch {
    hir::CaseBranch {
        coverage: hir::CaseBranchCoverage::Source,
        pattern: constructor_pattern(symbol, &[], span),
        value,
        span,
    }
}

fn constructor_pattern(
    symbol: SymbolId,
    arguments: &[hir::LocalBinder],
    span: TextRange,
) -> hir::Pattern {
    hir::Pattern {
        kind: hir::PatternKind::Constructor {
            symbol,
            name_span: span,
            arguments: arguments
                .iter()
                .map(|binder| hir::Pattern {
                    kind: hir::PatternKind::Var(binder.clone()),
                    span,
                })
                .collect(),
        },
        span,
    }
}

fn nullary_constructor(declaration: &hir::TypeDeclaration, name: &str) -> Option<SymbolId> {
    declaration
        .constructors
        .iter()
        .find(|constructor| constructor.name == name && constructor.fields.is_empty())
        .map(|constructor| constructor.symbol)
}

fn ordering_result_id(mut ty: &hir::Type) -> Option<hir::TypeId> {
    loop {
        match &ty.kind {
            hir::TypeKind::Forall { body, .. } | hir::TypeKind::Constrained { body, .. } => {
                ty = body;
            }
            hir::TypeKind::Function { result, .. } => ty = result,
            hir::TypeKind::Named(id) => return Some(*id),
            _ => return None,
        }
    }
}

fn local_expr(local: LocalId, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Local(local),
        span,
    }
}

fn global_expr(symbol: SymbolId, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Global(symbol),
        span,
    }
}

fn apply_expr(function: hir::Expr, argument: hir::Expr, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Application(Box::new(function), Box::new(argument)),
        span,
    }
}
