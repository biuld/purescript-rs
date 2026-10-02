use crate::expr::Desugarer;
use psrs_hir::{CaseBranch, Expr, ExprKind, LocalBinding, PatternKind};
use psrs_span::TextRange;

pub(super) fn lower_boolean_case(
    desugarer: &mut Desugarer,
    scrutinee: Expr,
    branches: Vec<CaseBranch>,
    span: TextRange,
) -> Expr {
    let temp = desugarer.local_binder("boolean_scrutinee", span);
    let mut next = desugarer.failure(span);
    for branch in branches.into_iter().rev() {
        let failure = desugarer.clone_expression(&next);
        let branch_value = desugarer.branch_value(branch.value, failure);
        match branch.pattern.kind {
            PatternKind::Wildcard => next = branch_value,
            PatternKind::Var(binder) => {
                next = Expr {
                    kind: ExprKind::Let {
                        bindings: vec![LocalBinding {
                            binder,
                            value: desugarer.local_expr(&temp, branch.span),
                            span: branch.span,
                        }],
                        body: Box::new(branch_value),
                    },
                    span: branch.span,
                };
            }
            PatternKind::Boolean(true) => {
                next = Expr {
                    kind: ExprKind::If {
                        condition: Box::new(desugarer.local_expr(&temp, branch.span)),
                        then_branch: Box::new(branch_value),
                        else_branch: Box::new(desugarer.clone_expression(&next)),
                    },
                    span: branch.span,
                };
            }
            PatternKind::Boolean(false) => {
                next = Expr {
                    kind: ExprKind::If {
                        condition: Box::new(desugarer.local_expr(&temp, branch.span)),
                        then_branch: Box::new(desugarer.clone_expression(&next)),
                        else_branch: Box::new(branch_value),
                    },
                    span: branch.span,
                };
            }
            PatternKind::Constructor { .. } | PatternKind::Record { .. } => {
                unreachable!("boolean case rows were checked before lowering")
            }
        }
    }
    Expr {
        kind: ExprKind::Let {
            bindings: vec![LocalBinding {
                binder: temp,
                value: scrutinee,
                span,
            }],
            body: Box::new(next),
        },
        span,
    }
}
