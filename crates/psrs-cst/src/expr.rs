use super::{CstName, Declaration, DeclarationBlock, Guard, TypeExpr};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Name(CstName),
    Integer(String),
    Number(String),
    String(String),
    Char(char),
    Hole(String),
    Array {
        open_bracket_span: TextRange,
        elements: Vec<Expr>,
        close_bracket_span: TextRange,
    },
    Record {
        open_brace_span: TextRange,
        fields: Vec<RecordField>,
        tail: Option<Box<Expr>>,
        close_brace_span: TextRange,
    },
    RecordUpdate {
        expression: Box<Expr>,
        open_brace_span: TextRange,
        fields: Vec<RecordUpdateField>,
        close_brace_span: TextRange,
    },
    Application(Box<Expr>, Box<Expr>),
    Typed {
        expression: Box<Expr>,
        double_colon_span: TextRange,
        type_expr: TypeExpr,
    },
    FieldAccess {
        expression: Box<Expr>,
        dot_span: TextRange,
        field: CstName,
    },
    Operator {
        operator: CstName,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Negate {
        minus_span: TextRange,
        expression: Box<Expr>,
    },
    Lambda {
        backslash_span: TextRange,
        parameters: Vec<Pattern>,
        arrow_span: TextRange,
        body: Box<Expr>,
    },
    Let {
        let_keyword_span: TextRange,
        declarations: Vec<Declaration>,
        layout_start_span: TextRange,
        layout_end_span: TextRange,
        in_keyword_span: TextRange,
        body: Box<Expr>,
    },
    If {
        if_keyword_span: TextRange,
        condition: Box<Expr>,
        then_keyword_span: TextRange,
        then_branch: Box<Expr>,
        else_keyword_span: TextRange,
        else_branch: Box<Expr>,
    },
    Case {
        case_keyword_span: TextRange,
        scrutinees: Vec<Expr>,
        of_keyword_span: TextRange,
        layout_start_span: TextRange,
        alternatives: Vec<CaseAlternative>,
        layout_end_span: TextRange,
    },
    Do {
        do_keyword_span: TextRange,
        layout_start_span: TextRange,
        statements: Vec<DoStatement>,
        layout_end_span: TextRange,
        in_keyword_span: Option<TextRange>,
        result: Option<Box<Expr>>,
    },
    Parens {
        open_paren_span: TextRange,
        expression: Box<Expr>,
        close_paren_span: TextRange,
    },
    Tuple {
        open_paren_span: TextRange,
        items: Vec<Expr>,
        close_paren_span: TextRange,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordField {
    pub label: CstName,
    pub colon_span: TextRange,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordUpdateField {
    pub label: CstName,
    pub equals_span: TextRange,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseAlternative {
    pub patterns: Vec<Pattern>,
    pub rhs: CaseRhs,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaseRhs {
    Plain {
        arrow_span: TextRange,
        value: Expr,
        where_block: Option<DeclarationBlock>,
    },
    Guarded(Vec<GuardedCaseRhs>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardedCaseRhs {
    pub bar_span: TextRange,
    pub guards: Vec<Guard>,
    pub arrow_span: TextRange,
    pub value: Expr,
    pub where_block: Option<DeclarationBlock>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DoStatement {
    Let {
        let_keyword_span: TextRange,
        declarations: Vec<Declaration>,
        layout_start_span: TextRange,
        layout_end_span: TextRange,
    },
    Bind {
        pattern: Pattern,
        left_arrow_span: TextRange,
        value: Expr,
    },
    Discard(Expr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard(TextRange),
    Var(CstName),
    Constructor {
        name: CstName,
        arguments: Vec<Pattern>,
    },
    Integer(String),
    String(String),
    Char(char),
    Boolean(bool),
    Array {
        open_bracket_span: TextRange,
        elements: Vec<Pattern>,
        close_bracket_span: TextRange,
    },
    Tuple {
        open_paren_span: TextRange,
        elements: Vec<Pattern>,
        close_paren_span: TextRange,
    },
    Record {
        open_brace_span: TextRange,
        fields: Vec<RecordPatternField>,
        tail: Option<CstName>,
        close_brace_span: TextRange,
    },
    Parens {
        open_paren_span: TextRange,
        pattern: Box<Pattern>,
        close_paren_span: TextRange,
    },
    Named {
        name: CstName,
        at_span: TextRange,
        pattern: Box<Pattern>,
    },
    Typed {
        pattern: Box<Pattern>,
        double_colon_span: TextRange,
        type_expr: TypeExpr,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordPatternField {
    pub label: CstName,
    pub value: Option<(TextRange, Pattern)>,
    pub span: TextRange,
}
