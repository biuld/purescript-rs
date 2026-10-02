use crate::{
    case_helpers::{pattern_for_scrutinee, pattern_scrutinee},
    expr::Desugarer,
};
use psrs_hir::{
    CaseBranch, CaseBranchCoverage, Expr, ExprKind, Guard, GuardedExpr, Intrinsic, Pattern,
    PatternKind,
};
use psrs_span::TextRange;

impl Desugarer {
    pub(super) fn branch_value(&mut self, expression: Expr, failure: Expr) -> Expr {
        let span = expression.span;
        match expression.kind {
            ExprKind::Guarded(clauses) => self.clauses(clauses, failure, span),
            ExprKind::Let { bindings, body } => Expr {
                kind: ExprKind::Let {
                    bindings: bindings
                        .into_iter()
                        .map(|binding| self.binding(binding))
                        .collect(),
                    body: Box::new(self.branch_value(*body, failure)),
                },
                span,
            },
            kind => self.expr(Expr { kind, span }),
        }
    }

    pub(super) fn clauses(
        &mut self,
        clauses: Vec<GuardedExpr>,
        failure: Expr,
        span: TextRange,
    ) -> Expr {
        let mut next = failure;
        for clause in clauses.into_iter().rev() {
            let clause_span = clause.span;
            let mut value = self.expr(clause.value);
            value = self.guards(clause.guards, value, next, clause_span);
            if !clause.where_bindings.is_empty() {
                value = Expr {
                    kind: ExprKind::Let {
                        bindings: clause
                            .where_bindings
                            .into_iter()
                            .map(|binding| self.binding(binding))
                            .collect(),
                        body: Box::new(value),
                    },
                    span: clause_span,
                };
            }
            next = value;
        }
        Expr { span, ..next }
    }

    fn guards(
        &mut self,
        guards: Vec<Guard>,
        success: Expr,
        failure: Expr,
        span: TextRange,
    ) -> Expr {
        let mut next = success;
        for guard in guards.into_iter().rev() {
            let guard_span = match &guard {
                Guard::Boolean(expression) => expression.span,
                Guard::Pattern { pattern, .. } => pattern.span,
                Guard::Let { span, .. } => *span,
            };
            next = match guard {
                Guard::Boolean(condition) => Expr {
                    kind: ExprKind::If {
                        condition: Box::new(self.expr(condition)),
                        then_branch: Box::new(next),
                        else_branch: Box::new(self.clone_expression(&failure)),
                    },
                    span: guard_span,
                },
                Guard::Pattern { pattern, value } => {
                    let failure = self.clone_expression(&failure);
                    self.pattern_guard(pattern, value, next, failure, guard_span)
                }
                Guard::Let { bindings, span } => Expr {
                    kind: ExprKind::Let {
                        bindings: bindings
                            .into_iter()
                            .map(|binding| self.binding(binding))
                            .collect(),
                        body: Box::new(next),
                    },
                    span,
                },
            };
        }
        Expr { span, ..next }
    }

    fn pattern_guard(
        &mut self,
        pattern: Pattern,
        value: Expr,
        success: Expr,
        failure: Expr,
        span: TextRange,
    ) -> Expr {
        if let PatternKind::Boolean(expected) = &pattern.kind {
            let expected = *expected;
            let value = self.expr(value);
            let condition = if expected {
                value
            } else {
                let function = Expr {
                    kind: ExprKind::Global(Intrinsic::BooleanNot.symbol()),
                    span,
                };
                Expr {
                    kind: ExprKind::Application(Box::new(function), Box::new(value)),
                    span,
                }
            };
            return Expr {
                kind: ExprKind::If {
                    condition: Box::new(condition),
                    then_branch: Box::new(success),
                    else_branch: Box::new(failure),
                },
                span,
            };
        }
        let scrutinee = pattern_scrutinee(self.expr(value), &pattern);
        self.expr(Expr {
            kind: ExprKind::Case {
                scrutinee: Box::new(scrutinee),
                branches: vec![
                    CaseBranch {
                        pattern: pattern_for_scrutinee(pattern.clone()),
                        value: success,
                        span,
                        coverage: CaseBranchCoverage::Generated,
                    },
                    CaseBranch {
                        pattern: Pattern {
                            kind: PatternKind::Wildcard,
                            span,
                        },
                        value: failure,
                        span,
                        coverage: CaseBranchCoverage::Generated,
                    },
                ],
            },
            span,
        })
    }
}
