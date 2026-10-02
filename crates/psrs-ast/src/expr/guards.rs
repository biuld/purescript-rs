use super::{Binder, Declaration, Expr, ExprKind, LowerError, Name, Pattern, PatternKind};
use crate::{lower_declarations, lower_expr, tuple_label};
use psrs_cst as cst;
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardedExpr {
    pub guards: Vec<Guard>,
    pub value: Expr,
    pub where_declarations: Vec<Declaration>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Guard {
    Boolean(Expr),
    Pattern {
        pattern: Pattern,
        value: Expr,
    },
    Let {
        declarations: Vec<Declaration>,
        span: TextRange,
    },
}

pub(crate) fn lower_guard(guard: cst::Guard) -> Result<Guard, LowerError> {
    match guard {
        cst::Guard::Boolean(expression) => Ok(Guard::Boolean(lower_expr(expression)?)),
        cst::Guard::Pattern { pattern, value, .. } => Ok(Guard::Pattern {
            pattern: super::lower_pattern(pattern)?,
            value: lower_expr(value)?,
        }),
        cst::Guard::Let {
            let_keyword_span,
            layout_end_span,
            declarations,
            ..
        } => Ok(Guard::Let {
            declarations: lower_declarations(declarations)?,
            span: TextRange::new(let_keyword_span.start, layout_end_span.end),
        }),
    }
}

pub(crate) fn lower_guarded_rhs(
    clauses: Vec<cst::GuardedRhs>,
) -> Result<Vec<GuardedExpr>, LowerError> {
    clauses
        .into_iter()
        .map(|clause| {
            Ok(GuardedExpr {
                guards: clause
                    .guards
                    .into_iter()
                    .map(lower_guard)
                    .collect::<Result<Vec<_>, _>>()?,
                value: lower_expr(clause.value)?,
                where_declarations: Vec::new(),
                span: clause.span,
            })
        })
        .collect()
}

pub(crate) fn lower_case_patterns(
    patterns: Vec<cst::Pattern>,
    span: TextRange,
) -> Result<(Pattern, Vec<Guard>), LowerError> {
    if patterns.is_empty() {
        return Err(LowerError::new(
            span,
            "case alternatives require at least one pattern",
        ));
    }
    if let Some(error) = crate::check_argument_names(&patterns) {
        return Err(error);
    }
    let mut guards = Vec::new();
    if patterns.len() == 1 {
        let (pattern, guard) = lower_case_argument(patterns.into_iter().next().unwrap(), 0, span)?;
        guards.extend(guard);
        return Ok((pattern, guards));
    }
    let fields = patterns
        .into_iter()
        .enumerate()
        .map(|(index, pattern)| {
            let (pattern, guard) = lower_case_argument(pattern, index, span)?;
            guards.extend(guard);
            Ok((tuple_label(index), pattern))
        })
        .collect::<Result<Vec<_>, LowerError>>()?;
    Ok((
        Pattern {
            kind: PatternKind::Record { fields },
            span,
        },
        guards,
    ))
}

fn lower_case_argument(
    pattern: cst::Pattern,
    index: usize,
    alternative_span: TextRange,
) -> Result<(Pattern, Vec<Guard>), LowerError> {
    if let cst::PatternKind::Integer(value) = pattern.kind {
        let binder_name = format!("$psrs_case_literal_{}_{}", alternative_span.start, index);
        let binder = Binder {
            name: binder_name.clone(),
            span: pattern.span,
        };
        let span = pattern.span;
        let guard = equality_guard(binder_name, value, span);
        return Ok((
            Pattern {
                kind: PatternKind::Var(binder),
                span,
            },
            vec![guard],
        ));
    }
    Ok((super::lower_pattern(pattern)?, Vec::new()))
}

pub(crate) fn equality_guard(name: String, integer: String, span: TextRange) -> Guard {
    let left = Expr {
        kind: ExprKind::Name(Name { text: name, span }),
        span,
    };
    let right = Expr {
        kind: ExprKind::Integer(integer),
        span,
    };
    Guard::Boolean(Expr {
        kind: ExprKind::Operator {
            operator: Name {
                text: "==".into(),
                span,
            },
            left: Box::new(left),
            right: Box::new(right),
        },
        span,
    })
}

pub(crate) fn prepend_guards(mut expression: Expr, guards: Vec<Guard>, span: TextRange) -> Expr {
    if guards.is_empty() {
        return expression;
    }
    match expression.kind {
        ExprKind::Guarded(ref mut clauses) => {
            for clause in clauses {
                let mut prefixed = guards.clone();
                prefixed.append(&mut clause.guards);
                clause.guards = prefixed;
            }
            expression
        }
        ExprKind::Let { declarations, body } => Expr {
            kind: ExprKind::Let {
                declarations,
                body: Box::new(prepend_guards(*body, guards, span)),
            },
            span: expression.span,
        },
        kind => Expr {
            kind: ExprKind::Guarded(vec![GuardedExpr {
                guards,
                value: Expr {
                    kind,
                    span: expression.span,
                },
                where_declarations: Vec::new(),
                span,
            }]),
            span,
        },
    }
}

pub(crate) fn lower_case_scrutinees(
    scrutinees: Vec<psrs_cst::Expr>,
    span: TextRange,
) -> Result<Expr, LowerError> {
    match scrutinees.len() {
        0 => Err(LowerError::new(
            span,
            "a case expression requires a scrutinee",
        )),
        1 => lower_expr(scrutinees.into_iter().next().expect("one scrutinee")),
        _ => Ok(Expr {
            kind: ExprKind::Record(
                scrutinees
                    .into_iter()
                    .enumerate()
                    .map(|(index, expression)| Ok((tuple_label(index), lower_expr(expression)?)))
                    .collect::<Result<Vec<_>, LowerError>>()?,
            ),
            span,
        }),
    }
}
