use crate::{LowerError, Name, Type};
use psrs_cst::{RecordField, RecordUpdateField};
use psrs_span::TextRange;

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
    let pattern = super::lower_pattern(pattern)?;
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
