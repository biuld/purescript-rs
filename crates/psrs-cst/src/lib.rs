use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub module_keyword_span: TextRange,
    pub name: CstName,
    pub where_keyword_span: TextRange,
    pub declarations: Vec<Declaration>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CstName {
    pub text: String,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CstBinder {
    pub name: CstName,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub name: CstName,
    pub parameters: Vec<CstBinder>,
    pub equals_span: TextRange,
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
    Name(CstName),
    Integer(String),
    String(String),
    Char(char),
    Application(Box<Expr>, Box<Expr>),
    Operator {
        operator: CstName,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Lambda {
        backslash_span: TextRange,
        parameters: Vec<CstBinder>,
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
    Parens {
        open_paren_span: TextRange,
        expression: Box<Expr>,
        close_paren_span: TextRange,
    },
}
