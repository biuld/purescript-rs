use crate::{case_helpers::apply, expr::Desugarer};
use psrs_hir::{
    CaseBranch, CaseBranchCoverage, Expr, ExprKind, Intrinsic, LocalBinding, Pattern, PatternKind,
};
use psrs_span::TextRange;
use std::collections::HashSet;

/// Handles Boolean literals in direct fields of record and multi-scrutinee
/// patterns. The remaining pattern forms stay on the shared case path.
pub(super) fn supports(branches: &[CaseBranch]) -> bool {
    let mut has_boolean = false;
    branches.iter().all(|branch| match &branch.pattern.kind {
        PatternKind::Record { fields } => fields.iter().all(|(_, pattern)| match &pattern.kind {
            PatternKind::Boolean(_) => {
                has_boolean = true;
                true
            }
            PatternKind::Wildcard | PatternKind::Var(_) => true,
            PatternKind::Constructor { .. } | PatternKind::Record { .. } => false,
        }),
        PatternKind::Wildcard | PatternKind::Var(_) => true,
        PatternKind::Boolean(_) | PatternKind::Constructor { .. } => false,
    }) && has_boolean
}

pub(super) fn lower(
    desugarer: &mut Desugarer,
    scrutinee: Expr,
    branches: Vec<CaseBranch>,
    span: TextRange,
) -> Expr {
    let mut labels = branches
        .iter()
        .flat_map(|branch| match &branch.pattern.kind {
            PatternKind::Record { fields } => fields
                .iter()
                .filter_map(|(label, pattern)| {
                    matches!(pattern.kind, PatternKind::Boolean(_)).then_some(label.clone())
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    labels.retain(|label| seen.insert(label.clone()));

    let exhaustive_rows = branches
        .iter()
        .filter(|branch| {
            branch.coverage != CaseBranchCoverage::Guarded
                || crate::case_helpers::guarded_rhs_exhaustive(&branch.value, desugarer)
        })
        .collect::<Vec<_>>();
    if !covers_all(&exhaustive_rows, &labels) {
        desugarer.errors.push(psrs_hir::VerifyError {
            span,
            message: "non-exhaustive case; missing Boolean alternative",
        });
    }

    let temporary = desugarer.local_binder("boolean_product_scrutinee", span);
    let mut next = desugarer.failure(span);
    for branch in branches.into_iter().rev() {
        let fallback = desugarer.clone_expression(&next);
        let mut tests = Vec::new();
        let pattern = bind_boolean_fields(&branch.pattern, desugarer, &mut tests);
        let mut value = desugarer.branch_value(branch.value, fallback);
        for (condition, condition_span) in tests.into_iter().rev() {
            value = Expr {
                kind: ExprKind::If {
                    condition: Box::new(condition),
                    then_branch: Box::new(value),
                    else_branch: Box::new(desugarer.clone_expression(&next)),
                },
                span: condition_span,
            };
        }
        let wildcard_value = desugarer.clone_expression(&next);
        next = desugarer.expr(Expr {
            kind: ExprKind::Case {
                scrutinee: Box::new(Expr {
                    kind: ExprKind::Local(temporary.id),
                    span: branch.span,
                }),
                branches: vec![
                    CaseBranch {
                        pattern,
                        value,
                        span: branch.span,
                        coverage: CaseBranchCoverage::Generated,
                    },
                    CaseBranch {
                        pattern: Pattern {
                            kind: PatternKind::Wildcard,
                            span: branch.span,
                        },
                        value: wildcard_value,
                        span: branch.span,
                        coverage: CaseBranchCoverage::Generated,
                    },
                ],
            },
            span: branch.span,
        });
    }

    Expr {
        kind: ExprKind::Let {
            bindings: vec![LocalBinding {
                binder: temporary,
                value: desugarer.expr(scrutinee),
                span,
            }],
            body: Box::new(next),
        },
        span,
    }
}

fn covers_all(branches: &[&CaseBranch], labels: &[String]) -> bool {
    if labels.is_empty() {
        return !branches.is_empty();
    }
    [false, true].into_iter().all(|value| {
        let remaining = branches
            .iter()
            .copied()
            .filter(|branch| match boolean_at(&branch.pattern, &labels[0]) {
                Some(expected) => expected == value,
                None => true,
            })
            .collect::<Vec<_>>();
        covers_all(&remaining, &labels[1..])
    })
}

fn boolean_at(pattern: &Pattern, label: &str) -> Option<bool> {
    let PatternKind::Record { fields } = &pattern.kind else {
        return None;
    };
    fields.iter().find_map(|(field, pattern)| {
        (field == label).then(|| match pattern.kind {
            PatternKind::Boolean(value) => Some(value),
            PatternKind::Wildcard | PatternKind::Var(_) => None,
            PatternKind::Constructor { .. } | PatternKind::Record { .. } => unreachable!(
                "supported Boolean product rows contain only direct Boolean, variable, or wildcard fields"
            ),
        })
    })?
}

fn bind_boolean_fields(
    pattern: &Pattern,
    desugarer: &mut Desugarer,
    tests: &mut Vec<(Expr, TextRange)>,
) -> Pattern {
    let kind = match &pattern.kind {
        PatternKind::Record { fields } => PatternKind::Record {
            fields: fields
                .iter()
                .map(|(label, pattern)| {
                    (
                        label.clone(),
                        bind_boolean_fields(pattern, desugarer, tests),
                    )
                })
                .collect(),
        },
        PatternKind::Boolean(expected) => {
            let binder = desugarer.local_binder("boolean_pattern", pattern.span);
            let value = Expr {
                kind: ExprKind::Local(binder.id),
                span: pattern.span,
            };
            let condition = if *expected {
                value
            } else {
                apply(
                    Expr {
                        kind: ExprKind::Global(Intrinsic::BooleanNot.symbol()),
                        span: pattern.span,
                    },
                    [value],
                    pattern.span,
                )
            };
            tests.push((condition, pattern.span));
            PatternKind::Var(binder)
        }
        leaf @ (PatternKind::Wildcard | PatternKind::Var(_)) => leaf.clone(),
        PatternKind::Constructor { .. } => {
            unreachable!("supported Boolean product rows contain no constructors")
        }
    };
    Pattern {
        kind,
        span: pattern.span,
    }
}
