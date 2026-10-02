use crate::{
    alpha::{self, FreshLocals},
    boolean_case::lower_boolean_case,
    boolean_product_case,
    case_helpers::{
        apply, boolean_case_exhaustive, guarded_rhs_exhaustive, is_guarded_rhs, product_expression,
        product_pattern, wrap_lambdas,
    },
    free_vars,
};
use psrs_hir::{
    self as hir, CaseBranch, CaseBranchCoverage, Expr, ExprKind, LocalBinder, LocalBinding,
    Pattern, PatternKind, SymbolId,
};
use psrs_span::TextRange;
use std::collections::HashSet;

pub(super) struct Desugarer {
    fresh: FreshLocals,
    pub(super) true_symbols: HashSet<SymbolId>,
    pub(super) errors: Vec<hir::VerifyError>,
}

impl Desugarer {
    pub(super) fn new(module: &hir::Module, known_true_symbols: &HashSet<SymbolId>) -> Self {
        Self {
            fresh: FreshLocals::after_module(module),
            true_symbols: known_true_symbols.clone(),
            errors: Vec::new(),
        }
    }

    pub(super) fn lower(&mut self, expression: Expr) -> Expr {
        self.expr(expression)
    }

    pub(super) fn expr(&mut self, expression: Expr) -> Expr {
        let span = expression.span;
        let kind = match expression.kind {
            ExprKind::OperatorChain { .. } | ExprKind::OperatorSection { .. } => {
                unreachable!("P4 fixity normalization runs before guard and case lowering")
            }
            ExprKind::Operator {
                operator,
                operator_span,
                left,
                right,
            } => {
                let function = Expr {
                    kind: ExprKind::Global(operator),
                    span: operator_span,
                };
                let left = self.expr(*left);
                let right = self.expr(*right);
                let partial = Expr {
                    kind: ExprKind::Application(Box::new(function), Box::new(left)),
                    span,
                };
                ExprKind::Application(Box::new(partial), Box::new(right))
            }
            ExprKind::Negate {
                function,
                expression,
                ..
            } => ExprKind::Application(
                Box::new(self.expr(*function)),
                Box::new(self.expr(*expression)),
            ),
            ExprKind::Array(elements) => {
                ExprKind::Array(elements.into_iter().map(|item| self.expr(item)).collect())
            }
            ExprKind::Record(fields) => ExprKind::Record(
                fields
                    .into_iter()
                    .map(|(label, value)| (label, self.expr(value)))
                    .collect(),
            ),
            ExprKind::RecordUpdate { expression, fields } => ExprKind::RecordUpdate {
                expression: Box::new(self.expr(*expression)),
                fields: fields
                    .into_iter()
                    .map(|(label, value)| (label, self.expr(value)))
                    .collect(),
            },
            ExprKind::FieldAccess { expression, field } => ExprKind::FieldAccess {
                expression: Box::new(self.expr(*expression)),
                field,
            },
            ExprKind::Application(function, argument) => ExprKind::Application(
                Box::new(self.expr(*function)),
                Box::new(self.expr(*argument)),
            ),
            ExprKind::Typed { expression, ty } => ExprKind::Typed {
                expression: Box::new(self.expr(*expression)),
                ty,
            },
            ExprKind::Lambda { binder, body } => ExprKind::Lambda {
                binder,
                body: Box::new(self.expr(*body)),
            },
            ExprKind::Let { bindings, body } => ExprKind::Let {
                bindings: bindings
                    .into_iter()
                    .map(|binding| self.binding(binding))
                    .collect(),
                body: Box::new(self.expr(*body)),
            },
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => ExprKind::If {
                condition: Box::new(self.expr(*condition)),
                then_branch: Box::new(self.expr(*then_branch)),
                else_branch: Box::new(self.expr(*else_branch)),
            },
            ExprKind::Case {
                scrutinee,
                branches,
            } => return self.case(*scrutinee, branches, span),
            ExprKind::Guarded(clauses) => {
                let failure = self.failure(span);
                return self.clauses(clauses, failure, span);
            }
            leaf @ (ExprKind::Local(_)
            | ExprKind::Global(_)
            | ExprKind::Integer(_)
            | ExprKind::Number(_)
            | ExprKind::String(_)
            | ExprKind::Char(_)) => leaf,
        };
        Expr { kind, span }
    }

    fn case(&mut self, scrutinee: Expr, mut branches: Vec<CaseBranch>, span: TextRange) -> Expr {
        let mut scrutinee = self.expr(scrutinee);
        let has_guards = branches.iter().any(|branch| is_guarded_rhs(&branch.value));
        let boolean_matrix = branches
            .iter()
            .any(|branch| matches!(branch.pattern.kind, PatternKind::Boolean(_)))
            && branches.iter().all(|branch| {
                matches!(
                    branch.pattern.kind,
                    PatternKind::Boolean(_) | PatternKind::Wildcard | PatternKind::Var(_)
                )
            });
        if boolean_matrix {
            if !boolean_case_exhaustive(&branches, self) {
                self.errors.push(hir::VerifyError {
                    span,
                    message: "non-exhaustive case; missing Boolean alternative",
                });
            }
            return lower_boolean_case(self, scrutinee, branches, span);
        }
        if boolean_product_case::supports(&branches) {
            return boolean_product_case::lower(self, scrutinee, branches, span);
        }
        if !has_guards {
            return Expr {
                kind: ExprKind::Case {
                    scrutinee: Box::new(scrutinee),
                    branches: branches
                        .into_iter()
                        .map(|mut branch| {
                            branch.value = self.expr(branch.value);
                            branch
                        })
                        .collect(),
                },
                span,
            };
        }

        if !branches
            .iter()
            .all(|branch| matches!(branch.pattern.kind, PatternKind::Record { .. }))
        {
            scrutinee = product_expression(scrutinee);
            for branch in &mut branches {
                branch.pattern = product_pattern(branch.pattern.clone());
            }
        }

        let captures = free_vars::captures(&branches);
        let temp = self.local_binder("case_scrutinee", span);
        let helper_binders = (1..=branches.len() + 1)
            .map(|index| self.local_binder(&format!("guard_fallthrough_{index}"), span))
            .collect::<Vec<_>>();
        let mut bindings = Vec::with_capacity(helper_binders.len() + 1);
        bindings.push(LocalBinding {
            binder: temp.clone(),
            value: scrutinee,
            span,
        });

        for row_index in 0..=branches.len() {
            let next_functions = helper_binders[row_index + 1..]
                .iter()
                .map(|_| self.local_binder("next_guard", span))
                .collect::<Vec<_>>();
            let capture_parameters = captures
                .iter()
                .map(|_| self.local_binder("guard_capture", span))
                .collect::<Vec<_>>();
            let temp_parameter = self.local_binder("guard_scrutinee", span);
            let mut local_mapping = captures
                .iter()
                .zip(&capture_parameters)
                .map(|(id, binder)| (*id, binder.id))
                .collect::<std::collections::HashMap<_, _>>();
            local_mapping.insert(temp.id, temp_parameter.id);

            let body = if row_index == branches.len() {
                Expr {
                    kind: ExprKind::Case {
                        scrutinee: Box::new(self.local_expr(&temp_parameter, span)),
                        branches: Vec::new(),
                    },
                    span,
                }
            } else {
                let mut source = alpha::clone_branch(&branches[row_index], &mut self.fresh);
                source.coverage = CaseBranchCoverage::Generated;
                source.value = free_vars::rebind(source.value, &local_mapping);
                let failure = apply(
                    self.local_expr(&next_functions[0], span),
                    next_functions[1..]
                        .iter()
                        .map(|binder| self.local_expr(binder, span))
                        .chain(
                            capture_parameters
                                .iter()
                                .map(|binder| self.local_expr(binder, span)),
                        )
                        .chain(std::iter::once(self.local_expr(&temp_parameter, span))),
                    span,
                );
                let guard_failure = self.clone_expression(&failure);
                source.value = self.branch_value(source.value, guard_failure);
                let wildcard = CaseBranch {
                    pattern: Pattern {
                        kind: PatternKind::Wildcard,
                        span: source.span,
                    },
                    value: self.clone_expression(&failure),
                    span: source.span,
                    coverage: CaseBranchCoverage::Generated,
                };
                Expr {
                    kind: ExprKind::Case {
                        scrutinee: Box::new(self.local_expr(&temp_parameter, span)),
                        branches: vec![source, wildcard],
                    },
                    span,
                }
            };
            let parameters = next_functions
                .into_iter()
                .chain(capture_parameters)
                .chain(std::iter::once(temp_parameter))
                .collect::<Vec<_>>();
            let value = wrap_lambdas(parameters, body, span);
            bindings.push(LocalBinding {
                binder: helper_binders[row_index].clone(),
                value,
                span,
            });
        }

        let root_branches = branches
            .into_iter()
            .enumerate()
            .map(|(index, mut branch)| {
                if branch.coverage == CaseBranchCoverage::Guarded
                    && guarded_rhs_exhaustive(&branch.value, self)
                {
                    branch.coverage = CaseBranchCoverage::Source;
                }
                if is_guarded_rhs(&branch.value) {
                    let start = index + 2;
                    let failure = apply(
                        self.local_expr(&helper_binders[start - 1], branch.span),
                        helper_binders[start..]
                            .iter()
                            .map(|binder| self.local_expr(binder, branch.span))
                            .chain(captures.iter().map(|id| Expr {
                                kind: ExprKind::Local(*id),
                                span: branch.span,
                            }))
                            .chain(std::iter::once(self.local_expr(&temp, branch.span))),
                        branch.span,
                    );
                    branch.value = self.branch_value(branch.value, failure);
                } else {
                    branch.value = self.expr(branch.value);
                }
                branch
            })
            .collect();
        let body = Expr {
            kind: ExprKind::Case {
                scrutinee: Box::new(self.local_expr(&temp, span)),
                branches: root_branches,
            },
            span,
        };
        Expr {
            kind: ExprKind::Let {
                bindings,
                body: Box::new(body),
            },
            span,
        }
    }

    pub(super) fn binding(&mut self, binding: LocalBinding) -> LocalBinding {
        LocalBinding {
            value: self.expr(binding.value),
            ..binding
        }
    }

    pub(super) fn failure(&mut self, span: TextRange) -> Expr {
        Expr {
            kind: ExprKind::Case {
                scrutinee: Box::new(Expr {
                    kind: ExprKind::Record(Vec::new()),
                    span,
                }),
                branches: Vec::new(),
            },
            span,
        }
    }

    pub(super) fn local_expr(&self, binder: &LocalBinder, span: TextRange) -> Expr {
        Expr {
            kind: ExprKind::Local(binder.id),
            span,
        }
    }

    pub(super) fn clone_expression(&mut self, expression: &Expr) -> Expr {
        alpha::clone_expression(expression, &mut self.fresh)
    }

    pub(super) fn local_binder(&mut self, hint: &str, span: TextRange) -> LocalBinder {
        LocalBinder {
            id: self.fresh.alloc(),
            name: format!("$psrs_{hint}_{}", span.start),
            span,
        }
    }
}
