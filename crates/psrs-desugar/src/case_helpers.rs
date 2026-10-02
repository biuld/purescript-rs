use crate::expr::Desugarer;
use psrs_hir::{Expr, ExprKind, Guard, LocalBinder, Pattern, PatternKind};
use psrs_span::TextRange;

pub(super) fn apply(
    function: Expr,
    arguments: impl IntoIterator<Item = Expr>,
    span: TextRange,
) -> Expr {
    arguments
        .into_iter()
        .fold(function, |function, argument| Expr {
            kind: ExprKind::Application(Box::new(function), Box::new(argument)),
            span,
        })
}

pub(super) fn wrap_lambdas(parameters: Vec<LocalBinder>, body: Expr, span: TextRange) -> Expr {
    parameters
        .into_iter()
        .rev()
        .fold(body, |body, binder| Expr {
            kind: ExprKind::Lambda {
                binder,
                body: Box::new(body),
            },
            span,
        })
}

pub(super) fn product_expression(value: Expr) -> Expr {
    let span = value.span;
    Expr {
        kind: ExprKind::Record(vec![("_1".into(), value)]),
        span,
    }
}

pub(super) fn product_pattern(pattern: Pattern) -> Pattern {
    let span = pattern.span;
    Pattern {
        kind: PatternKind::Record {
            fields: vec![("_1".into(), pattern)],
            mode: psrs_hir::RecordPatternMode::Exact,
        },
        span,
    }
}

pub(super) fn pattern_scrutinee(value: Expr, pattern: &Pattern) -> Expr {
    // A record pattern consumes its value directly, regardless of whether the
    // source expression happened to be written as a record literal. Other
    // pattern guards share the generated one-column product with case rows.
    if matches!(pattern.kind, PatternKind::Record { .. }) {
        value
    } else {
        product_expression(value)
    }
}

pub(super) fn pattern_for_scrutinee(pattern: Pattern) -> Pattern {
    if matches!(pattern.kind, PatternKind::Record { .. }) {
        pattern
    } else {
        product_pattern(pattern)
    }
}

pub(super) fn is_guarded_rhs(expression: &Expr) -> bool {
    match &expression.kind {
        ExprKind::Guarded(_) => true,
        ExprKind::Let { body, .. } => is_guarded_rhs(body),
        _ => false,
    }
}

pub(super) fn guarded_rhs_exhaustive(expression: &Expr, desugarer: &Desugarer) -> bool {
    match &expression.kind {
        ExprKind::Guarded(clauses) => clauses.iter().any(|clause| {
            clause
                .guards
                .iter()
                .all(|guard| guard_is_exhaustive(guard, desugarer))
        }),
        ExprKind::Let { body, .. } => guarded_rhs_exhaustive(body, desugarer),
        _ => false,
    }
}

fn guard_is_exhaustive(guard: &Guard, desugarer: &Desugarer) -> bool {
    match guard {
        Guard::Boolean(expression) => is_true_expression(expression, desugarer),
        Guard::Pattern { pattern, .. } => pattern_is_irrefutable(pattern),
        Guard::Let { .. } => true,
    }
}

fn is_true_expression(expression: &Expr, desugarer: &Desugarer) -> bool {
    match &expression.kind {
        ExprKind::Global(symbol) => desugarer.true_symbols.contains(symbol),
        ExprKind::Typed { expression, .. } => is_true_expression(expression, desugarer),
        _ => false,
    }
}

fn pattern_is_irrefutable(pattern: &Pattern) -> bool {
    match &pattern.kind {
        PatternKind::Wildcard | PatternKind::Var(_) => true,
        PatternKind::Named { pattern, .. } | PatternKind::Typed { pattern, .. } => {
            pattern_is_irrefutable(pattern)
        }
        PatternKind::Record { fields, .. } => fields
            .iter()
            .all(|(_, field)| pattern_is_irrefutable(field)),
        PatternKind::Array(_)
        | PatternKind::Integer(_)
        | PatternKind::Number(_)
        | PatternKind::String(_)
        | PatternKind::Char(_)
        | PatternKind::Boolean(_)
        | PatternKind::Constructor { .. }
        | PatternKind::OperatorChain { .. } => false,
    }
}
