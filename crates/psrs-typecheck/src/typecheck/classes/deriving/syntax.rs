//! The one term builder every structural deriving rule uses.
//!
//! A rule states its traversal; the lambdas, cases, applications, and literals
//! it emits are constructed here, so no rule builds resolved HIR by hand. The
//! output is ordinary resolved HIR and is checked by the normal inference path.

use super::super::super::*;

pub(super) fn local_expr(local: LocalId, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Local(local),
        span,
    }
}

pub(super) fn global_expr(symbol: SymbolId, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Global(symbol),
        span,
    }
}

pub(super) fn apply_expr(function: hir::Expr, argument: hir::Expr, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Application(Box::new(function), Box::new(argument)),
        span,
    }
}

pub(super) fn boolean_literal(value: bool, span: TextRange) -> hir::Expr {
    global_expr(
        if value {
            hir::Intrinsic::BoolTrue.symbol()
        } else {
            hir::Intrinsic::BoolFalse.symbol()
        },
        span,
    )
}

pub(super) fn lambda(binder: hir::LocalBinder, body: hir::Expr, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Lambda {
            binder,
            body: Box::new(body),
        },
        span,
    }
}

pub(super) fn case_expr(
    value: hir::Expr,
    branches: Vec<hir::CaseBranch>,
    span: TextRange,
) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::Case {
            scrutinee: Box::new(value),
            branches,
        },
        span,
    }
}

pub(super) fn constructor_pattern(
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
                .map(|binder| hir::Pattern {
                    kind: hir::PatternKind::Var(binder.clone()),
                    span,
                })
                .collect(),
        },
        span,
    }
}

pub(super) fn field_expr(value: hir::Expr, field: &str, span: TextRange) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::FieldAccess {
            expression: Box::new(value),
            field: field.into(),
        },
        span,
    }
}

pub(super) fn record_update(
    value: hir::Expr,
    fields: Vec<(String, hir::Expr)>,
    span: TextRange,
) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::RecordUpdate {
            expression: Box::new(value),
            fields,
        },
        span,
    }
}

pub(super) fn if_expr(
    condition: hir::Expr,
    then_branch: hir::Expr,
    else_branch: hir::Expr,
    span: TextRange,
) -> hir::Expr {
    hir::Expr {
        kind: hir::ExprKind::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
        },
        span,
    }
}

pub(super) fn variable_pattern(binder: &hir::LocalBinder, span: TextRange) -> hir::Pattern {
    hir::Pattern {
        kind: hir::PatternKind::Var(binder.clone()),
        span,
    }
}

pub(super) fn wildcard_pattern(span: TextRange) -> hir::Pattern {
    hir::Pattern {
        kind: hir::PatternKind::Wildcard,
        span,
    }
}

pub(super) fn symbol_pattern(
    symbol: SymbolId,
    arguments: Vec<hir::Pattern>,
    span: TextRange,
) -> hir::Pattern {
    hir::Pattern {
        kind: hir::PatternKind::Constructor {
            symbol,
            name_span: span,
            arguments,
        },
        span,
    }
}
