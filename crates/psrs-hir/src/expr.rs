use super::{Associativity, LocalId, SymbolId, Type};
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
    /// A type ascription `e :: T`, with the written type already resolved. The
    /// checker elaborates it and keeps the expression with its checked type, so
    /// the node does not survive into Typed Core.
    Typed {
        expression: Box<Expr>,
        ty: Type,
    },
    Operator {
        operator: SymbolId,
        operator_span: TextRange,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// Unary minus after P3 has resolved its ordinary `negate` name.
    Negate {
        function: Box<Expr>,
        minus_span: TextRange,
        expression: Box<Expr>,
    },
    OperatorChain {
        operands: Vec<Expr>,
        operators: Vec<ResolvedOperator>,
    },
    OperatorSection {
        operator: ResolvedOperator,
        operand: Box<Expr>,
        binder: LocalBinder,
        side: SectionSide,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedOperator {
    pub symbol: SymbolId,
    pub operator_span: TextRange,
    pub associativity: Associativity,
    pub precedence: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionSide {
    Left,
    Right,
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
    OperatorChain {
        operands: Vec<Pattern>,
        operators: Vec<ResolvedOperator>,
    },
    Record {
        fields: Vec<(String, Pattern)>,
    },
}
