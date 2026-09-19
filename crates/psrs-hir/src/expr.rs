use super::{LocalId, SymbolId, Type};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub symbol: SymbolId,
    pub name: String,
    pub name_span: TextRange,
    pub value: Expr,
    pub signature: Option<Type>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalBinder {
    pub id: LocalId,
    pub name: String,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalBinding {
    pub binder: LocalBinder,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Local(LocalId),
    Global(SymbolId),
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
        operator: SymbolId,
        operator_span: TextRange,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Lambda {
        binder: LocalBinder,
        body: Box<Expr>,
    },
    Let {
        bindings: Vec<LocalBinding>,
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

/// One alternative of a `case` expression. The pattern binds locals that are in
/// scope only in the branch value.
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
    Var(LocalBinder),
    /// A data constructor pattern, resolved to the constructor's value symbol.
    Constructor {
        symbol: SymbolId,
        name_span: TextRange,
        arguments: Vec<Pattern>,
    },
}
