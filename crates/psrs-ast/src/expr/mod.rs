use crate::{LowerError, Name, Type};
use psrs_cst as cst;
use psrs_span::TextRange;
use std::collections::HashSet;

mod guards;
mod records;
pub use guards::{Guard, GuardedExpr};
pub(super) use guards::{
    lower_case_patterns, lower_case_scrutinees, lower_guard, lower_guarded_rhs, lower_if,
    prepend_guards,
};
pub(super) use records::{lower_record, lower_record_update};

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
        fields: Vec<RecordUpdateField>,
    },
    FieldAccess {
        expression: Box<Expr>,
        field: String,
    },
    Application(Box<Expr>, Box<Expr>),
    /// A type ascription `e :: T`. The written type is a type-directed check at
    /// a known expression, so it is elaborated by the checker and the
    /// expression is kept with its checked type; nothing is wrapped at runtime.
    Typed {
        expression: Box<Expr>,
        ty: Type,
    },
    /// A visible type application `e @T`. It selects a type argument that the
    /// expression's own quantifiers would otherwise leave to inference, so the
    /// checker instantiates that quantifier with the written type and keeps the
    /// expression with the resulting type. Like [`ExprKind::Typed`], it is
    /// erased at runtime: the value is the inner expression.
    TypeApplication {
        expression: Box<Expr>,
        ty: Type,
    },
    Operator {
        operator: Name,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Negate {
        minus_span: TextRange,
        expression: Box<Expr>,
    },
    OperatorChain {
        operands: Vec<Expr>,
        operators: Vec<super::Operator>,
    },
    OperatorSection {
        operator: Name,
        operand: Box<Expr>,
        side: super::SectionSide,
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
    /// A guarded declaration RHS. P4 expands clauses to ordered conditions and
    /// cases after the resolver has assigned IDs to pattern and let guards.
    Guarded(Vec<GuardedExpr>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordUpdateField {
    pub label: String,
    pub value: RecordUpdateValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordUpdateValue {
    Expression(Expr),
    /// A source path update, whose base is the enclosing record's field.
    Nested {
        fields: Vec<RecordUpdateField>,
        span: TextRange,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordPatternMode {
    /// A source record pattern selects named fields and accepts additional fields.
    Partial,
    /// A tuple or compiler product matches the complete closed record shape.
    Exact,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Boolean(bool),
    Integer(String),
    Number(String),
    String(String),
    Char(char),
    Array {
        elements: Vec<Pattern>,
    },
    Var(Binder),
    /// A data constructor pattern; the name is resolved in the value namespace.
    Constructor {
        name: Name,
        arguments: Vec<Pattern>,
    },
    OperatorChain {
        operands: Vec<Pattern>,
        operators: Vec<super::Operator>,
    },
    Record {
        fields: Vec<(String, Pattern)>,
        mode: RecordPatternMode,
    },
    Named {
        binder: Binder,
        pattern: Box<Pattern>,
    },
    Typed {
        pattern: Box<Pattern>,
        ty: crate::Type,
    },
}

pub(super) fn lower_field_access(
    expression: psrs_cst::Expr,
    field: psrs_cst::CstName,
    span: TextRange,
) -> Result<Expr, LowerError> {
    Ok(Expr {
        kind: ExprKind::FieldAccess {
            expression: Box::new(super::lower_expr(expression)?),
            field: field.text,
        },
        span,
    })
}

pub(super) fn lower_record_accessor(
    marker_span: TextRange,
    fields: Vec<psrs_cst::RecordAccessorField>,
    span: TextRange,
) -> Expr {
    let binder = Binder {
        name: format!("$psrs_record_accessor_{}", marker_span.start),
        span: marker_span,
    };
    let mut body = Expr {
        kind: ExprKind::Name(crate::Name {
            text: binder.name.clone(),
            span: marker_span,
        }),
        span: marker_span,
    };
    for field in fields {
        body = Expr {
            kind: ExprKind::FieldAccess {
                expression: Box::new(body),
                field: field.field.text,
            },
            span: TextRange::new(marker_span.start, field.field.span.end),
        };
    }
    let mut accessor = super::lower_lambda(binder, body);
    accessor.span = span;
    accessor
}

pub(super) fn lower_pattern_lambda(
    pattern: psrs_cst::Pattern,
    body: Expr,
) -> Result<Expr, LowerError> {
    if let Some(error) = crate::check_argument_names(std::slice::from_ref(&pattern)) {
        return Err(error);
    }
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
        let name = format!("$psrs_wildcard_{}", span.start);
        return Ok(super::lower_lambda(Binder { name, span }, body));
    }
    let span = pattern.span;
    let name = format!("$psrs_pattern_{}", span.start);
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
                    name.span,
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
        cst::PatternKind::OperatorChain { operands, .. } => {
            for operand in operands {
                if let Some(error) = check_pattern_names(operand, seen) {
                    return Some(error);
                }
            }
        }
        cst::PatternKind::Array { elements, .. } => {
            for element in elements {
                if let Some(error) = check_pattern_names(element, seen) {
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
        cst::PatternKind::Record { fields, tail, .. } => {
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
            if let Some(tail) = tail
                && !seen.insert(tail.text.clone())
            {
                return Some(LowerError::coded(
                    tail.span,
                    "OverlappingArgNames",
                    "two arguments share the same name",
                ));
            }
        }
        cst::PatternKind::Named { name, pattern, .. } => {
            if !seen.insert(name.text.clone()) {
                return Some(LowerError::coded(
                    name.span,
                    "OverlappingArgNames",
                    "two arguments share the same name",
                ));
            }
            return check_pattern_names(pattern, seen);
        }
        cst::PatternKind::Typed { pattern, .. } => {
            return check_pattern_names(pattern, seen);
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
        cst::PatternKind::Boolean(value) => PatternKind::Boolean(value),
        cst::PatternKind::Integer(value) => PatternKind::Integer(value),
        cst::PatternKind::Number(value) => PatternKind::Number(value),
        cst::PatternKind::String(value) => PatternKind::String(value),
        cst::PatternKind::Char(value) => PatternKind::Char(value),
        cst::PatternKind::Array { elements, .. } => PatternKind::Array {
            elements: elements
                .into_iter()
                .map(lower_pattern)
                .collect::<Result<_, _>>()?,
        },
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
        cst::PatternKind::OperatorChain {
            operands,
            operators,
        } => PatternKind::OperatorChain {
            operands: operands
                .into_iter()
                .map(lower_pattern)
                .collect::<Result<_, _>>()?,
            operators: operators
                .into_iter()
                .map(|name| super::Operator {
                    span: name.span,
                    name: super::lower_name(name),
                })
                .collect(),
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
                mode: RecordPatternMode::Partial,
            }
        }
        cst::PatternKind::Tuple { elements, .. } => PatternKind::Record {
            fields: elements
                .into_iter()
                .enumerate()
                .map(|(index, element)| Ok((super::tuple_label(index), lower_pattern(element)?)))
                .collect::<Result<_, _>>()?,
            mode: RecordPatternMode::Exact,
        },
        cst::PatternKind::Parens { pattern, .. } => {
            let mut lowered = lower_pattern(*pattern)?;
            lowered.span = span;
            return Ok(lowered);
        }
        cst::PatternKind::Named { name, pattern, .. } => PatternKind::Named {
            binder: Binder {
                name: name.text,
                span: name.span,
            },
            pattern: Box::new(lower_pattern(*pattern)?),
        },
        cst::PatternKind::Typed {
            pattern, type_expr, ..
        } => PatternKind::Typed {
            pattern: Box::new(lower_pattern(*pattern)?),
            ty: super::lower_type(type_expr)?,
        },
    };
    Ok(Pattern { kind, span })
}
