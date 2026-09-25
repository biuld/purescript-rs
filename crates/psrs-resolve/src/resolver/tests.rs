use super::*;
use psrs_ast::{
    Binder, Declaration as AstDeclaration, ExprKind as AstExprKind, Name, Type as AstType,
    TypeKind as AstTypeKind,
};
use psrs_hir::{
    BuiltinType, ExprKind, Intrinsic, LocalId, ModuleId, SymbolId, Type as HirType,
    TypeKind as HirTypeKind,
};

fn name(text: &str, start: u32) -> Name {
    Name {
        text: text.into(),
        span: TextRange::new(start, start + text.len() as u32),
    }
}

fn expression(kind: AstExprKind, start: u32, end: u32) -> ast::Expr {
    ast::Expr {
        kind,
        span: TextRange::new(start, end),
    }
}

fn declaration(name_text: &str, start: u32, value: ast::Expr) -> AstDeclaration {
    AstDeclaration {
        name: name(name_text, start),
        span: TextRange::new(start, value.span.end),
        value,
        annotation: None,
    }
}

fn type_name(text: &str, start: u32) -> AstType {
    AstType {
        kind: AstTypeKind::Name(name(text, start)),
        span: TextRange::new(start, start + text.len() as u32),
    }
}

fn module(declarations: Vec<AstDeclaration>) -> ast::Module {
    ast::Module {
        name: name("Main", 7),
        exports: None,
        imports: Vec::new(),
        declarations,
        foreign_imports: Vec::new(),
        type_declarations: Vec::new(),
        span: TextRange::new(0, 100),
    }
}

#[test]
fn resolves_forward_globals_and_lambda_locals_to_stable_ids() {
    let module = module(vec![
        declaration(
            "first",
            19,
            expression(AstExprKind::Name(name("second", 27)), 27, 33),
        ),
        declaration(
            "second",
            35,
            expression(
                AstExprKind::Lambda {
                    binder: Binder {
                        name: "value".into(),
                        span: TextRange::new(42, 47),
                    },
                    body: Box::new(expression(AstExprKind::Name(name("value", 51)), 51, 56)),
                },
                41,
                56,
            ),
        ),
    ]);

    let resolved = resolve_module(module, ModuleId(3)).unwrap();
    assert_eq!(
        resolved.declarations[0].symbol,
        SymbolId::new(ModuleId(3), 0)
    );
    assert!(matches!(
        resolved.declarations[0].value.kind,
        ExprKind::Global(symbol) if symbol == SymbolId::new(ModuleId(3), 1)
    ));
    let ExprKind::Lambda { binder, body } = &resolved.declarations[1].value.kind else {
        panic!("expected resolved lambda");
    };
    assert_eq!(binder.id, LocalId(0));
    assert!(matches!(body.kind, ExprKind::Local(id) if id == binder.id));
    resolved.verify().unwrap();
}

#[test]
fn resolves_bootstrap_integer_operator_to_intrinsic_id() {
    let module = module(vec![declaration(
        "main",
        19,
        expression(
            AstExprKind::Operator {
                operator: name("+", 29),
                left: Box::new(expression(AstExprKind::Integer("40".into()), 27, 29)),
                right: Box::new(expression(AstExprKind::Integer("2".into()), 32, 33)),
            },
            27,
            33,
        ),
    )]);

    let resolved =
        resolve_module_with_externals(module, ModuleId(0), &bootstrap_externals()).unwrap();
    assert!(matches!(
        resolved.declarations[0].value.kind,
        ExprKind::Operator { operator, .. } if operator == Intrinsic::I32Add.symbol()
    ));
    resolved.verify().unwrap();
}

#[test]
fn let_bindings_are_mutually_visible_and_capture_outer_locals() {
    let let_expression = expression(
        AstExprKind::Let {
            declarations: vec![
                declaration(
                    "first",
                    55,
                    expression(AstExprKind::Name(name("second", 63)), 63, 69),
                ),
                declaration(
                    "second",
                    71,
                    expression(AstExprKind::Name(name("x", 80)), 80, 81),
                ),
            ],
            body: Box::new(expression(AstExprKind::Name(name("first", 85)), 85, 90)),
        },
        51,
        90,
    );
    let value = expression(
        AstExprKind::Lambda {
            binder: Binder {
                name: "x".into(),
                span: TextRange::new(45, 46),
            },
            body: Box::new(let_expression),
        },
        44,
        90,
    );
    let resolved =
        resolve_module(module(vec![declaration("main", 19, value)]), ModuleId(0)).unwrap();
    let ExprKind::Lambda { binder, body } = &resolved.declarations[0].value.kind else {
        panic!("expected lambda");
    };
    let ExprKind::Let { bindings, body } = &body.kind else {
        panic!("expected let");
    };
    assert!(matches!(
        bindings[0].value.kind,
        ExprKind::Local(id) if id == bindings[1].binder.id
    ));
    assert!(matches!(
        bindings[1].value.kind,
        ExprKind::Local(id) if id == binder.id
    ));
    assert!(matches!(
        body.kind,
        ExprKind::Local(id) if id == bindings[0].binder.id
    ));
    resolved.verify().unwrap();
}

#[test]
fn inner_let_binding_shadows_an_outer_lambda_binder() {
    let value = expression(
        AstExprKind::Lambda {
            binder: Binder {
                name: "x".into(),
                span: TextRange::new(24, 25),
            },
            body: Box::new(expression(
                AstExprKind::Let {
                    declarations: vec![declaration(
                        "x",
                        35,
                        expression(AstExprKind::Integer("1".into()), 39, 40),
                    )],
                    body: Box::new(expression(AstExprKind::Name(name("x", 44)), 44, 45)),
                },
                31,
                45,
            )),
        },
        23,
        45,
    );
    let resolved =
        resolve_module(module(vec![declaration("main", 19, value)]), ModuleId(0)).unwrap();
    let ExprKind::Lambda { binder, body } = &resolved.declarations[0].value.kind else {
        panic!("expected lambda");
    };
    let ExprKind::Let { bindings, body } = &body.kind else {
        panic!("expected let");
    };
    assert_ne!(binder.id, bindings[0].binder.id);
    assert!(matches!(body.kind, ExprKind::Local(id) if id == bindings[0].binder.id));
    resolved.verify().unwrap();
}

#[test]
fn reports_unknown_names_with_the_name_span() {
    let module = module(vec![declaration(
        "main",
        19,
        expression(AstExprKind::Name(name("missing", 26)), 26, 33),
    )]);

    let errors = resolve_module(module, ModuleId(0)).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, ResolveErrorKind::UnknownName);
    assert_eq!(errors[0].span, TextRange::new(26, 33));
}

#[test]
fn reports_a_missing_qualified_module_instead_of_dropping_the_declaration() {
    let qualified_name = "Missing.value";
    let module = module(vec![declaration(
        "main",
        19,
        expression(
            AstExprKind::Name(name(qualified_name, 26)),
            26,
            26 + qualified_name.len() as u32,
        ),
    )]);

    let errors = resolve_module(module, ModuleId(0)).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, ResolveErrorKind::UnknownName);
    assert_eq!(errors[0].span, TextRange::new(26, 39));
    assert!(errors[0].message().contains("Missing.value"));
}

#[test]
fn rejects_duplicate_top_level_names() {
    let module = module(vec![
        declaration(
            "main",
            19,
            expression(AstExprKind::Integer("1".into()), 26, 27),
        ),
        declaration(
            "main",
            28,
            expression(AstExprKind::Integer("2".into()), 35, 36),
        ),
    ]);

    let errors = resolve_module(module, ModuleId(0)).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.kind == ResolveErrorKind::DuplicateDeclaration && error.span == TextRange::new(28, 32)
    }));
}

#[test]
fn rejects_duplicate_bindings_in_one_let_group() {
    let module = module(vec![declaration(
        "main",
        19,
        expression(
            AstExprKind::Let {
                declarations: vec![
                    declaration(
                        "value",
                        30,
                        expression(AstExprKind::Integer("1".into()), 38, 39),
                    ),
                    declaration(
                        "value",
                        40,
                        expression(AstExprKind::Integer("2".into()), 48, 49),
                    ),
                ],
                body: Box::new(expression(AstExprKind::Name(name("value", 53)), 53, 58)),
            },
            26,
            58,
        ),
    )]);

    let errors = resolve_module(module, ModuleId(0)).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.kind == ResolveErrorKind::DuplicateLocalBinding
            && error.span == TextRange::new(40, 45)
    }));
}

#[test]
fn resolves_declaration_signature_to_hir_type() {
    let annotation = AstType {
        kind: AstTypeKind::Function {
            parameter: Box::new(type_name("Int", 27)),
            result: Box::new(type_name("Boolean", 32)),
        },
        span: TextRange::new(27, 39),
    };
    let mut declaration = declaration(
        "main",
        19,
        expression(AstExprKind::Integer("1".into()), 43, 44),
    );
    declaration.annotation = Some(annotation);

    let resolved = resolve_module(module(vec![declaration]), ModuleId(0)).unwrap();
    assert_eq!(
        resolved.declarations[0].signature,
        Some(HirType {
            kind: HirTypeKind::Function {
                parameter: Box::new(HirType {
                    kind: HirTypeKind::Constructor(BuiltinType::Int),
                    span: TextRange::new(27, 30),
                }),
                result: Box::new(HirType {
                    kind: HirTypeKind::Constructor(BuiltinType::Boolean),
                    span: TextRange::new(32, 39),
                }),
            },
            span: TextRange::new(27, 39),
        })
    );
}

#[test]
fn reports_unknown_uppercase_type_names_with_the_name_span() {
    let mut declaration = declaration(
        "main",
        19,
        expression(AstExprKind::Integer("1".into()), 34, 35),
    );
    declaration.annotation = Some(type_name("Maybe", 27));

    let errors = resolve_module(module(vec![declaration]), ModuleId(0)).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, ResolveErrorKind::UnknownTypeName);
    assert_eq!(errors[0].span, TextRange::new(27, 32));
    assert_eq!(errors[0].message(), "unknown type name `Maybe`");
}
