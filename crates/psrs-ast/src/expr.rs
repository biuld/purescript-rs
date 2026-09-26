use crate::{LowerError, Name, Type};
use psrs_cst::{self as cst, RecordField, RecordUpdateField};
use psrs_span::TextRange;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binder {
    pub name: String,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub name: Name,
    pub value: Expr,
    pub span: TextRange,
    pub annotation: Option<Type>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Name(Name),
    Integer(String),
    Number(String),
    String(String),
    Char(char),
    Array(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    RecordUpdate {
        expression: Box<Expr>,
        fields: Vec<(String, Expr)>,
    },
    FieldAccess {
        expression: Box<Expr>,
        field: String,
    },
    Application(Box<Expr>, Box<Expr>),
    Operator {
        operator: Name,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Lambda {
        binder: Binder,
        body: Box<Expr>,
    },
    Let {
        declarations: Vec<Declaration>,
        body: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    Case {
        scrutinee: Box<Expr>,
        branches: Vec<CaseBranch>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseBranch {
    pub pattern: Pattern,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Var(Binder),
    /// A data constructor pattern; the name is resolved in the value namespace.
    Constructor {
        name: Name,
        arguments: Vec<Pattern>,
    },
    Record {
        fields: Vec<(String, Pattern)>,
    },
}

pub(super) fn lower_record(
    fields: Vec<RecordField>,
    tail: Option<Box<psrs_cst::Expr>>,
    span: TextRange,
) -> Result<ExprKind, LowerError> {
    if tail.is_some() {
        return Err(LowerError::new(
            span,
            "open record rows are not supported yet",
        ));
    }
    Ok(ExprKind::Record(
        fields
            .into_iter()
            .map(|field| Ok((field.label.text, super::lower_expr(field.value)?)))
            .collect::<Result<Vec<_>, LowerError>>()?,
    ))
}

pub(super) fn lower_record_update(
    expression: psrs_cst::Expr,
    fields: Vec<RecordUpdateField>,
) -> Result<ExprKind, LowerError> {
    Ok(ExprKind::RecordUpdate {
        expression: Box::new(super::lower_expr(expression)?),
        fields: fields
            .into_iter()
            .map(|field| Ok((field.label.text, super::lower_expr(field.value)?)))
            .collect::<Result<Vec<_>, LowerError>>()?,
    })
}

pub(super) fn lower_pattern_lambda(
    pattern: psrs_cst::Pattern,
    body: Expr,
) -> Result<Expr, LowerError> {
    if let psrs_cst::PatternKind::Var(name) = &pattern.kind {
        return Ok(super::lower_lambda(
            Binder {
                name: name.text.clone(),
                span: name.span,
            },
            body,
        ));
    }
    // A wildcard parameter always matches, so it needs no case. Lowering it to
    // a case would demand a data-type scrutinee and reject an ignored
    // parameter whose type has no pattern-matching representation, such as the
    // token-taking continuation in an effect `bind`.
    if let psrs_cst::PatternKind::Wildcard(_) = &pattern.kind {
        let span = pattern.span;
        let name = format!("__psrs_wildcard_{}", span.start);
        return Ok(super::lower_lambda(Binder { name, span }, body));
    }
    let span = pattern.span;
    let name = format!("__psrs_pattern_{}", span.start);
    let binder = Binder {
        name: name.clone(),
        span,
    };
    let scrutinee = Expr {
        kind: ExprKind::Name(Name { text: name, span }),
        span,
    };
    let pattern = lower_pattern(pattern)?;
    let case = Expr {
        kind: ExprKind::Case {
            scrutinee: Box::new(scrutinee),
            branches: vec![CaseBranch {
                pattern,
                value: body.clone(),
                span: TextRange::new(span.start, body.span.end),
            }],
        },
        span: TextRange::new(span.start, body.span.end),
    };
    Ok(super::lower_lambda(binder, case))
}

pub(super) fn check_pattern_names(
    pattern: &cst::Pattern,
    seen: &mut HashSet<String>,
) -> Option<LowerError> {
    match &pattern.kind {
        cst::PatternKind::Var(name) => {
            if !seen.insert(name.text.clone()) {
                return Some(LowerError::coded(
                    pattern.span,
                    "OverlappingArgNames",
                    "two arguments share the same name",
                ));
            }
        }
        cst::PatternKind::Constructor { arguments, .. } => {
            for argument in arguments {
                if let Some(error) = check_pattern_names(argument, seen) {
                    return Some(error);
                }
            }
        }
        cst::PatternKind::Tuple { elements, .. } => {
            for element in elements {
                if let Some(error) = check_pattern_names(element, seen) {
                    return Some(error);
                }
            }
        }
        cst::PatternKind::Record { fields, .. } => {
            for field in fields {
                if let Some((_, pattern)) = &field.value {
                    if let Some(error) = check_pattern_names(pattern, seen) {
                        return Some(error);
                    }
                } else if !seen.insert(field.label.text.clone()) {
                    return Some(LowerError::coded(
                        field.label.span,
                        "OverlappingArgNames",
                        "two arguments share the same name",
                    ));
                }
            }
        }
        cst::PatternKind::Parens { pattern, .. } => {
            return check_pattern_names(pattern, seen);
        }
        _ => {}
    }
    None
}

/// Lowers a pattern in a `case` alternative or binder position.
pub(super) fn lower_pattern(pattern: cst::Pattern) -> Result<Pattern, LowerError> {
    let span = pattern.span;
    let kind = match pattern.kind {
        cst::PatternKind::Wildcard(_) => PatternKind::Wildcard,
        cst::PatternKind::Var(name) => PatternKind::Var(Binder {
            name: name.text,
            span: name.span,
        }),
        cst::PatternKind::Constructor { name, arguments } => PatternKind::Constructor {
            name: super::lower_name(name),
            arguments: arguments
                .into_iter()
                .map(lower_pattern)
                .collect::<Result<_, _>>()?,
        },
        cst::PatternKind::Record { fields, tail, .. } => {
            if tail.is_some() {
                return Err(LowerError::new(
                    span,
                    "open record patterns are not supported yet",
                ));
            }
            PatternKind::Record {
                fields: fields
                    .into_iter()
                    .map(|field| {
                        let pattern = field.value.map_or_else(
                            || {
                                Ok(Pattern {
                                    kind: PatternKind::Var(Binder {
                                        name: field.label.text.clone(),
                                        span: field.label.span,
                                    }),
                                    span: field.label.span,
                                })
                            },
                            |(_, pattern)| lower_pattern(pattern),
                        )?;
                        Ok((field.label.text, pattern))
                    })
                    .collect::<Result<_, LowerError>>()?,
            }
        }
        cst::PatternKind::Tuple { elements, .. } => PatternKind::Record {
            fields: elements
                .into_iter()
                .enumerate()
                .map(|(index, element)| Ok((super::tuple_label(index), lower_pattern(element)?)))
                .collect::<Result<_, _>>()?,
        },
        cst::PatternKind::Parens { pattern, .. } => {
            let mut lowered = lower_pattern(*pattern)?;
            lowered.span = span;
            return Ok(lowered);
        }
        _ => {
            return Err(LowerError::new(
                span,
                "this pattern syntax is not supported yet",
            ));
        }
    };
    Ok(Pattern { kind, span })
}
