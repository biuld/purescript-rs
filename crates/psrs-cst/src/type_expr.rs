use super::{CstName, TypeVarBinder};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeExpr {
    pub kind: TypeExprKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeExprKind {
    Name(CstName),
    Wildcard(TextRange),
    Integer(String),
    String(String),
    Function {
        left: Box<TypeExpr>,
        arrow_span: TextRange,
        right: Box<TypeExpr>,
    },
    Forall {
        forall_span: TextRange,
        variables: Vec<TypeVarBinder>,
        dot_span: TextRange,
        body: Box<TypeExpr>,
    },
    Constrained {
        constraint: Box<TypeExpr>,
        arrow_span: TextRange,
        body: Box<TypeExpr>,
    },
    Application(Box<TypeExpr>, Vec<TypeExpr>),
    Operator {
        operator: CstName,
        left: Box<TypeExpr>,
        right: Box<TypeExpr>,
    },
    PrefixOperator {
        operator: CstName,
        operand: Box<TypeExpr>,
    },
    Parens {
        open_paren_span: TextRange,
        expression: Box<TypeExpr>,
        close_paren_span: TextRange,
    },
    Tuple {
        open_paren_span: TextRange,
        items: Vec<TypeExpr>,
        close_paren_span: TextRange,
    },
    Row {
        open_paren_span: TextRange,
        fields: Vec<TypeField>,
        tail: Option<Box<TypeExpr>>,
        close_paren_span: TextRange,
    },
    Record {
        open_brace_span: TextRange,
        fields: Vec<TypeField>,
        tail: Option<Box<TypeExpr>>,
        close_brace_span: TextRange,
    },
    KindAnnotation {
        expression: Box<TypeExpr>,
        double_colon_span: TextRange,
        kind: Box<TypeExpr>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeField {
    pub label: CstName,
    pub double_colon_span: TextRange,
    pub type_expr: TypeExpr,
    pub span: TextRange,
}
