use crate::{Name, Type};
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
