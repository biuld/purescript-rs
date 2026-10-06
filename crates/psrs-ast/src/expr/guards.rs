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

pub(crate) fn lower_guard(guard: cst::Guard) -> Result<Vec<Guard>, LowerError> {
    match guard {
        cst::Guard::Boolean(expression) => Ok(vec![Guard::Boolean(lower_expr(expression)?)]),
        cst::Guard::Pattern { pattern, value, .. } => {
            let value = lower_expr(value)?;
            Ok(vec![Guard::Pattern {
                pattern: super::lower_pattern(pattern)?,
                value,
            }])
        }
        cst::Guard::Let {
            let_keyword_span,
            layout_end_span,
            declarations,
            ..
        } => Ok(vec![Guard::Let {
            declarations: lower_declarations(declarations)?,
            span: TextRange::new(let_keyword_span.start, layout_end_span.end),
        }]),
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
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .flatten()
                    .collect(),
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
            kind: PatternKind::Record {
                fields,
                mode: super::RecordPatternMode::Exact,
            },
            span,
        },
        guards,
    ))
}

fn lower_case_argument(
    pattern: cst::Pattern,
    _index: usize,
    _alternative_span: TextRange,
) -> Result<(Pattern, Vec<Guard>), LowerError> {
    Ok((super::lower_pattern(pattern)?, Vec::new()))
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

/// An immediate `_` in an `if` condition, then branch, or else branch is a
/// function parameter, in that written order. Nested expressions keep their
/// own underscores.
pub(crate) fn lower_if(
    condition: cst::Expr,
    then_branch: cst::Expr,
    else_branch: cst::Expr,
    span: TextRange,
) -> Result<Expr, LowerError> {
    let mut binders = Vec::new();
    let condition = lower_immediate_anonymous(condition, &mut binders)?;
    let then_branch = lower_immediate_anonymous(then_branch, &mut binders)?;
    let else_branch = lower_immediate_anonymous(else_branch, &mut binders)?;
    let mut expression = Expr {
        kind: ExprKind::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
        },
        span,
    };
    for binder in binders.into_iter().rev() {
        expression = Expr {
            kind: ExprKind::Lambda {
                binder,
                body: Box::new(expression),
            },
            span,
        };
    }
    Ok(expression)
}

fn lower_immediate_anonymous(
    expression: cst::Expr,
    binders: &mut Vec<Binder>,
) -> Result<Expr, LowerError> {
    if matches!(&expression.kind, cst::ExprKind::Name(name) if name.text == "_") {
        let span = expression.span;
        let name = format!("$psrs_if_argument_{}", span.start);
        binders.push(Binder {
            name: name.clone(),
            span,
        });
        return Ok(Expr {
            kind: ExprKind::Name(Name { text: name, span }),
            span,
        });
    }
    lower_expr(expression)
}

pub(crate) fn lower_case_scrutinees(
    scrutinees: Vec<psrs_cst::Expr>,
    span: TextRange,
) -> Result<(Expr, Vec<Binder>), LowerError> {
    if scrutinees.is_empty() {
        return Err(LowerError::new(
            span,
            "a case expression requires a scrutinee",
        ));
    }
    let mut anonymous = Vec::new();
    let mut lowered = scrutinees
        .into_iter()
        .enumerate()
        .map(|(index, expression)| {
            let is_anonymous =
                matches!(&expression.kind, cst::ExprKind::Name(name) if name.text == "_");
            let value = if is_anonymous {
                let binder = Binder {
                    name: format!("$psrs_case_input_{}_{}", span.start, index),
                    span: expression.span,
                };
                let value = Expr {
                    kind: ExprKind::Name(Name {
                        text: binder.name.clone(),
                        span: binder.span,
                    }),
                    span: expression.span,
                };
                anonymous.push(binder);
                value
            } else {
                lower_expr(expression)?
            };
            Ok((index, value))
        })
        .collect::<Result<Vec<_>, LowerError>>()?;
    let scrutinee = if lowered.len() == 1 {
        lowered.pop().expect("one case scrutinee").1
    } else {
        Expr {
            kind: ExprKind::Record(
                lowered
                    .into_iter()
                    .map(|(index, value)| (tuple_label(index), value))
                    .collect(),
            ),
            span,
        }
    };
    Ok((scrutinee, anonymous))
}
