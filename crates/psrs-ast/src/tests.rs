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
