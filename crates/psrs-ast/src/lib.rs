use psrs_cst::{self as cst, ExprKind as CstExprKind, TypeExprKind as CstTypeExprKind};
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
    pub annotation: Option<Type>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeKind {
    Name(Name),
    Function {
        parameter: Box<Type>,
        result: Box<Type>,
    },
    Forall {
        variables: Vec<String>,
        body: Box<Type>,
    },
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

/// A construct that parsed but has no AST lowering yet. Returning an error
/// instead of inventing a node keeps unsupported syntax from reaching later
/// passes, and lets the parser grow ahead of resolution and type checking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LowerError {
    pub span: TextRange,
    pub message: &'static str,
}

/// Converts source-oriented CST nodes into a normalized, unresolved surface AST.
pub fn lower_module(module: cst::Module) -> Result<Module, Vec<LowerError>> {
    let mut errors = Vec::new();
    let mut declarations = Vec::new();
    for declaration in module.declarations {
        match lower_declaration(declaration) {
            Ok(declaration) => declarations.push(declaration),
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        Ok(Module {
            name: lower_name(module.name),
            declarations,
            span: module.span,
        })
    } else {
        Err(errors)
    }
}

fn lower_declaration(declaration: cst::Declaration) -> Result<Declaration, LowerError> {
    let span = declaration.span();
    let cst::Declaration::Value(declaration) = declaration else {
        return Err(LowerError {
            span,
            message: "this declaration is not supported yet",
        });
    };
    let cst::ValueRhs::Plain { value, .. } = declaration.rhs else {
        return Err(LowerError {
            span: declaration.span,
            message: "guarded equations are not supported yet",
        });
    };
    if let Some(block) = declaration.where_block {
        return Err(LowerError {
            span: block.span,
            message: "where blocks are not supported yet",
        });
    }
    let mut value = lower_expr(value)?;
    for parameter in declaration.parameters.into_iter().rev() {
        value = lower_pattern_lambda(parameter, value)?;
    }
    Ok(Declaration {
        name: lower_name(declaration.name),
        value,
        span: declaration.span,
        annotation: declaration.annotation.map(lower_type).transpose()?,
    })
}

fn lower_pattern_lambda(pattern: cst::Pattern, body: Expr) -> Result<Expr, LowerError> {
    match pattern.kind {
        cst::PatternKind::Var(name) => Ok(lower_lambda(
            Binder {
                name: name.text,
                span: name.span,
            },
            body,
        )),
        cst::PatternKind::Parens { pattern, .. } => lower_pattern_lambda(*pattern, body),
        _ => Err(LowerError {
            span: pattern.span,
            message: "only variable binders are supported yet",
        }),
    }
}

fn lower_type(expression: cst::TypeExpr) -> Result<Type, LowerError> {
    let span = expression.span;
    let kind = match expression.kind {
        CstTypeExprKind::Name(name) => TypeKind::Name(lower_name(name)),
        CstTypeExprKind::Function { left, right, .. } => TypeKind::Function {
            parameter: Box::new(lower_type(*left)?),
            result: Box::new(lower_type(*right)?),
        },
        CstTypeExprKind::Forall {
            variables, body, ..
        } => TypeKind::Forall {
            variables: variables
                .into_iter()
                .map(|variable| variable.name.text)
                .collect(),
            body: Box::new(lower_type(*body)?),
        },
        CstTypeExprKind::Parens { expression, .. } => {
            let mut expression = lower_type(*expression)?;
            expression.span = span;
            return Ok(expression);
        }
        CstTypeExprKind::Wildcard(_)
        | CstTypeExprKind::Hole(_)
        | CstTypeExprKind::Integer(_)
        | CstTypeExprKind::String(_)
        | CstTypeExprKind::Constrained { .. }
        | CstTypeExprKind::Application(..)
        | CstTypeExprKind::Operator { .. }
        | CstTypeExprKind::PrefixOperator { .. }
        | CstTypeExprKind::Tuple { .. }
        | CstTypeExprKind::Row { .. }
        | CstTypeExprKind::Record { .. }
        | CstTypeExprKind::KindAnnotation { .. } => {
            return Err(LowerError {
                span,
                message: "this type syntax is not supported yet",
            });
        }
    };
    Ok(Type { kind, span })
}

fn lower_expr(expression: cst::Expr) -> Result<Expr, LowerError> {
    let span = expression.span;
    let kind = match expression.kind {
        CstExprKind::Name(name) => ExprKind::Name(lower_name(name)),
        CstExprKind::Integer(value) => ExprKind::Integer(value),
        CstExprKind::String(value) => ExprKind::String(value),
        CstExprKind::Char(value) => ExprKind::Char(value),
        CstExprKind::Application(function, argument) => ExprKind::Application(
            Box::new(lower_expr(*function)?),
            Box::new(lower_expr(*argument)?),
        ),
        CstExprKind::Operator {
            operator,
            left,
            right,
        } => ExprKind::Operator {
            operator: lower_name(operator),
            left: Box::new(lower_expr(*left)?),
            right: Box::new(lower_expr(*right)?),
        },
        CstExprKind::Lambda {
            parameters, body, ..
        } => {
            let mut body = lower_expr(*body)?;
            for parameter in parameters.into_iter().rev() {
                body = lower_pattern_lambda(parameter, body)?;
            }
            return Ok(Expr {
                kind: body.kind,
                span,
            });
        }
        CstExprKind::Let {
            declarations, body, ..
        } => {
            let mut lowered = Vec::with_capacity(declarations.len());
            for declaration in declarations {
                lowered.push(lower_declaration(declaration)?);
            }
            ExprKind::Let {
                declarations: lowered,
                body: Box::new(lower_expr(*body)?),
            }
        }
        CstExprKind::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => ExprKind::If {
            condition: Box::new(lower_expr(*condition)?),
            then_branch: Box::new(lower_expr(*then_branch)?),
            else_branch: Box::new(lower_expr(*else_branch)?),
        },
        CstExprKind::Parens { expression, .. } => {
            let mut expression = lower_expr(*expression)?;
            expression.span = span;
            return Ok(expression);
        }
        CstExprKind::Hole(_)
        | CstExprKind::Number(_)
        | CstExprKind::Array { .. }
        | CstExprKind::Record { .. }
        | CstExprKind::RecordUpdate { .. }
        | CstExprKind::FieldAccess { .. }
        | CstExprKind::Negate { .. }
        | CstExprKind::Case { .. }
        | CstExprKind::Do { .. }
        | CstExprKind::Tuple { .. }
        | CstExprKind::Typed { .. }
        | CstExprKind::TypeApplication { .. } => {
            return Err(LowerError {
                span,
                message: "this expression syntax is not supported yet",
            });
        }
    };
    Ok(Expr { kind, span })
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

fn lower_name(name: cst::CstName) -> Name {
    Name {
        text: name.text,
        span: name.span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_cst::{
        CstName, Declaration as CstDeclaration, Module as CstModule, Pattern, PatternKind,
        TypeVarBinder, ValueDeclaration, ValueRhs,
    };
    use psrs_span::TextRange;

    fn name(text: &str, start: u32) -> CstName {
        CstName {
            text: text.into(),
            span: TextRange::new(start, start + text.len() as u32),
        }
    }

    fn pattern_var(text: &str, start: u32) -> Pattern {
        Pattern {
            kind: PatternKind::Var(name(text, start)),
            span: TextRange::new(start, start + text.len() as u32),
        }
    }

    fn type_var_binder(text: &str, start: u32) -> TypeVarBinder {
        TypeVarBinder {
            name: name(text, start),
            kind: None,
            span: TextRange::new(start, start + text.len() as u32),
        }
    }

    fn cst_module(declaration: CstDeclaration) -> CstModule {
        CstModule {
            module_keyword_span: TextRange::new(0, 6),
            name: name("Main", 7),
            exports: None,
            where_keyword_span: TextRange::new(12, 17),
            imports: Vec::new(),
            declarations: vec![declaration],
            span: TextRange::new(0, 40),
        }
    }

    fn value_declaration(
        name_text: &str,
        name_start: u32,
        parameters: Vec<Pattern>,
        equals_span: TextRange,
        value: cst::Expr,
        span_end: u32,
        annotation: Option<cst::TypeExpr>,
    ) -> CstDeclaration {
        CstDeclaration::Value(ValueDeclaration {
            name: name(name_text, name_start),
            parameters,
            rhs: ValueRhs::Plain { equals_span, value },
            where_block: None,
            span: TextRange::new(name_start, span_end),
            annotation,
        })
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
        let declaration = value_declaration(
            "add",
            19,
            vec![pattern_var("x", 23), pattern_var("y", 25)],
            TextRange::new(27, 28),
            value,
            36,
            None,
        );
        let module = lower_module(cst_module(declaration)).unwrap();
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
        let declaration = value_declaration(
            "main",
            18,
            Vec::new(),
            TextRange::new(23, 24),
            cst::Expr {
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
            29,
            None,
        );
        let module = lower_module(cst_module(declaration)).unwrap();
        let value = &module.declarations[0].value;
        assert!(matches!(value.kind, ExprKind::Integer(ref value) if value == "42"));
        assert_eq!(value.span, TextRange::new(25, 29));
    }

    #[test]
    fn lowers_forall_types_and_removes_parentheses() {
        let annotation = cst::TypeExpr {
            kind: cst::TypeExprKind::Forall {
                forall_span: TextRange::new(0, 6),
                variables: vec![type_var_binder("a", 7)],
                dot_span: TextRange::new(8, 9),
                body: Box::new(cst::TypeExpr {
                    kind: cst::TypeExprKind::Parens {
                        open_paren_span: TextRange::new(10, 11),
                        expression: Box::new(cst::TypeExpr {
                            kind: cst::TypeExprKind::Name(name("a", 11)),
                            span: TextRange::new(11, 12),
                        }),
                        close_paren_span: TextRange::new(12, 13),
                    },
                    span: TextRange::new(10, 13),
                }),
            },
            span: TextRange::new(0, 13),
        };
        let declaration = value_declaration(
            "id",
            19,
            vec![pattern_var("x", 23)],
            TextRange::new(25, 26),
            cst::Expr {
                kind: CstExprKind::Name(name("x", 27)),
                span: TextRange::new(27, 28),
            },
            28,
            Some(annotation),
        );
        let module = lower_module(cst_module(declaration)).unwrap();
        let annotation = module.declarations[0].annotation.as_ref().unwrap();
        let TypeKind::Forall { variables, body } = &annotation.kind else {
            panic!("expected a lowered forall");
        };
        assert_eq!(variables, &["a".to_owned()]);
        assert!(matches!(&body.kind, TypeKind::Name(name) if name.text == "a"));
        assert_eq!(body.span, TextRange::new(10, 13));
    }
}
