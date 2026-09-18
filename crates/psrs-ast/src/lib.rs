use psrs_cst::{self as cst, CstBinder, ExprKind as CstExprKind};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: Name,
    pub declarations: Vec<Declaration>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub text: String,
    pub span: TextRange,
}

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
}

/// Converts source-oriented CST nodes into a normalized, unresolved surface AST.
pub fn lower_module(module: cst::Module) -> Module {
    Module {
        name: lower_name(module.name),
        declarations: module
            .declarations
            .into_iter()
            .map(lower_declaration)
            .collect(),
        span: module.span,
    }
}

fn lower_declaration(declaration: cst::Declaration) -> Declaration {
    let value = declaration
        .parameters
        .into_iter()
        .rev()
        .fold(lower_expr(declaration.value), |body, binder| {
            lower_lambda(lower_binder(binder), body)
        });
    Declaration {
        name: lower_name(declaration.name),
        value,
        span: declaration.span,
    }
}

fn lower_expr(expression: cst::Expr) -> Expr {
    let span = expression.span;
    let kind = match expression.kind {
        CstExprKind::Name(name) => ExprKind::Name(lower_name(name)),
        CstExprKind::Integer(value) => ExprKind::Integer(value),
        CstExprKind::String(value) => ExprKind::String(value),
        CstExprKind::Char(value) => ExprKind::Char(value),
        CstExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(lower_expr(*function)),
            Box::new(lower_expr(*argument)),
        ),
        CstExprKind::Operator {
            operator,
            left,
            right,
        } => ExprKind::Operator {
            operator: lower_name(operator),
            left: Box::new(lower_expr(*left)),
            right: Box::new(lower_expr(*right)),
        },
        CstExprKind::Lambda {
            parameters, body, ..
        } => {
            let body = parameters
                .into_iter()
                .rev()
                .fold(lower_expr(*body), |body, binder| {
                    lower_lambda(lower_binder(binder), body)
                });
            return Expr {
                kind: body.kind,
                span,
            };
        }
        CstExprKind::Let {
            declarations, body, ..
        } => ExprKind::Let {
            declarations: declarations.into_iter().map(lower_declaration).collect(),
            body: Box::new(lower_expr(*body)),
        },
        CstExprKind::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => ExprKind::If {
            condition: Box::new(lower_expr(*condition)),
            then_branch: Box::new(lower_expr(*then_branch)),
            else_branch: Box::new(lower_expr(*else_branch)),
        },
        CstExprKind::Parens { expression, .. } => {
            let mut expression = lower_expr(*expression);
            expression.span = span;
            return expression;
        }
    };
    Expr { kind, span }
}

fn lower_lambda(binder: Binder, body: Expr) -> Expr {
    let span = TextRange::new(binder.span.start, body.span.end);
    Expr {
        kind: ExprKind::Lambda {
            binder,
            body: Box::new(body),
        },
        span,
    }
}

fn lower_binder(binder: CstBinder) -> Binder {
    Binder {
        name: binder.name.text,
        span: binder.name.span,
    }
}

fn lower_name(name: cst::CstName) -> Name {
    Name {
        text: name.text,
        span: name.span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_cst::{CstBinder, CstName, Declaration as CstDeclaration, Module as CstModule};
    use psrs_span::TextRange;

    fn name(text: &str, start: u32) -> CstName {
        CstName {
            text: text.into(),
            span: TextRange::new(start, start + text.len() as u32),
        }
    }

    fn binder(text: &str, start: u32) -> CstBinder {
        CstBinder {
            name: name(text, start),
        }
    }

    fn cst_module(declaration: CstDeclaration) -> CstModule {
        CstModule {
            module_keyword_span: TextRange::new(0, 6),
            name: name("Main", 7),
            where_keyword_span: TextRange::new(12, 17),
            declarations: vec![declaration],
            span: TextRange::new(0, 40),
        }
    }

    #[test]
    fn function_parameters_become_nested_lambdas_and_names_stay_unresolved() {
        let value = cst::Expr {
            kind: CstExprKind::Operator {
                operator: name("+", 33),
                left: Box::new(cst::Expr {
                    kind: CstExprKind::Name(name("x", 31)),
                    span: TextRange::new(31, 32),
                }),
                right: Box::new(cst::Expr {
                    kind: CstExprKind::Name(name("y", 35)),
                    span: TextRange::new(35, 36),
                }),
            },
            span: TextRange::new(31, 36),
        };
        let declaration = CstDeclaration {
            name: name("add", 19),
            parameters: vec![binder("x", 23), binder("y", 25)],
            equals_span: TextRange::new(27, 28),
            value,
            span: TextRange::new(19, 36),
        };
        let module = lower_module(cst_module(declaration));
        let ExprKind::Lambda { binder, body } = &module.declarations[0].value.kind else {
            panic!("expected the first normalized lambda");
        };
        assert_eq!(binder.name, "x");
        let ExprKind::Lambda { binder, body } = &body.kind else {
            panic!("expected the second normalized lambda");
        };
        assert_eq!(binder.name, "y");
        let ExprKind::Operator { left, right, .. } = &body.kind else {
            panic!("expected the source operator to remain unresolved");
        };
        assert!(matches!(&left.kind, ExprKind::Name(name) if name.text == "x"));
        assert!(matches!(&right.kind, ExprKind::Name(name) if name.text == "y"));
    }

    #[test]
    fn parentheses_are_removed_without_losing_the_expression_range() {
        let declaration = CstDeclaration {
            name: name("main", 18),
            parameters: Vec::new(),
            equals_span: TextRange::new(23, 24),
            value: cst::Expr {
                kind: CstExprKind::Parens {
                    open_paren_span: TextRange::new(25, 26),
                    expression: Box::new(cst::Expr {
                        kind: CstExprKind::Integer("42".into()),
                        span: TextRange::new(26, 28),
                    }),
                    close_paren_span: TextRange::new(28, 29),
                },
                span: TextRange::new(25, 29),
            },
            span: TextRange::new(18, 29),
        };
        let module = lower_module(cst_module(declaration));
        let value = &module.declarations[0].value;
        assert!(matches!(value.kind, ExprKind::Integer(ref value) if value == "42"));
        assert_eq!(value.span, TextRange::new(25, 29));
    }
}
