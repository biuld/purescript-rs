use super::*;
use psrs_hir::{
    Declaration as HirDeclaration, Expr as HirExpr, ExprKind as HirExprKind, ModuleId, SymbolId,
    Type as HirType, TypeKind as HirTypeKind,
};
use psrs_resolve::bootstrap_externals;

fn expr(kind: HirExprKind, start: u32, end: u32) -> HirExpr {
    HirExpr {
        kind,
        span: TextRange::new(start, end),
    }
}

fn declaration(symbol: u32, name: &str, start: u32, value: HirExpr) -> HirDeclaration {
    HirDeclaration {
        symbol: SymbolId::new(ModuleId(0), symbol),
        name: name.into(),
        name_span: TextRange::new(start, start + name.len() as u32),
        span: TextRange::new(start, value.span.end),
        value,
        signature: None,
    }
}

fn declaration_with_signature(
    symbol: u32,
    name: &str,
    start: u32,
    signature: HirType,
    value: HirExpr,
) -> HirDeclaration {
    let mut declaration = declaration(symbol, name, start, value);
    declaration.signature = Some(signature);
    declaration
}

fn type_variable(name: &str, start: u32) -> HirType {
    HirType {
        kind: HirTypeKind::Variable(name.into()),
        span: TextRange::new(start, start + name.len() as u32),
    }
}

fn module(declarations: Vec<HirDeclaration>, with_intrinsics: bool) -> hir::Module {
    hir::Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: if with_intrinsics {
            bootstrap_externals()
        } else {
            Vec::new()
        },
        imports: Vec::new(),
        exports: None,
        declarations,
        types: Vec::new(),
        span: TextRange::new(0, 100),
    }
}

fn local(id: u32, start: u32) -> HirExpr {
    expr(HirExprKind::Local(LocalId(id)), start, start + 1)
}

fn integer(value: &str, start: u32) -> HirExpr {
    expr(
        HirExprKind::Integer(value.into()),
        start,
        start + value.len() as u32,
    )
}

#[test]
fn infers_functions_arithmetic_conditionals_and_intrinsic_booleans() {
    let add = Intrinsic::I32Add.symbol();
    let true_symbol = Intrinsic::BoolTrue.symbol();
    let increment = expr(
        HirExprKind::Lambda {
            binder: LocalBinder {
                id: LocalId(0),
                name: "value".into(),
                span: TextRange::new(24, 29),
            },
            body: Box::new(expr(
                HirExprKind::Operator {
                    operator: add,
                    operator_span: TextRange::new(34, 35),
                    left: Box::new(local(0, 32)),
                    right: Box::new(integer("1", 36)),
                },
                32,
                37,
            )),
        },
        23,
        37,
    );
    let main = expr(
        HirExprKind::If {
            condition: Box::new(expr(HirExprKind::Global(true_symbol), 48, 52)),
            then_branch: Box::new(expr(
                HirExprKind::Application(
                    Box::new(expr(
                        HirExprKind::Global(SymbolId::new(ModuleId(0), 0)),
                        58,
                        67,
                    )),
                    Box::new(integer("41", 68)),
                ),
                58,
                70,
            )),
            else_branch: Box::new(integer("0", 76)),
        },
        45,
        76,
    );
    let resolved = module(
        vec![
            declaration(0, "increment", 19, increment),
            declaration(1, "main", 40, main),
        ],
        true,
    );

    let resolved = psrs_desugar::desugar_module(resolved).unwrap();
    let typed = typecheck_module(resolved).unwrap();
    assert_eq!(
        typed.types[typed.declarations[0].ty.0 as usize],
        Type::Function {
            parameter: TypeId(0),
            result: TypeId(0),
        }
    );
    assert_eq!(typed.types[typed.declarations[1].ty.0 as usize], Type::I32);
    typed.verify().unwrap();
}

#[test]
fn rejects_a_non_boolean_if_condition() {
    let resolved = module(
        vec![declaration(
            0,
            "main",
            19,
            expr(
                HirExprKind::If {
                    condition: Box::new(integer("1", 26)),
                    then_branch: Box::new(integer("2", 35)),
                    else_branch: Box::new(integer("3", 44)),
                },
                23,
                45,
            ),
        )],
        false,
    );

    let errors = typecheck_module(resolved).unwrap_err();
    assert!(errors.iter().any(|error| {
        error.kind == TypeCheckErrorKind::TypeMismatch && error.span == TextRange::new(26, 27)
    }));
}

#[test]
fn generalizes_top_level_functions() {
    let identity = declaration(
        0,
        "identity",
        19,
        expr(
            HirExprKind::Lambda {
                binder: LocalBinder {
                    id: LocalId(0),
                    name: "value".into(),
                    span: TextRange::new(28, 33),
                },
                body: Box::new(local(0, 36)),
            },
            27,
            37,
        ),
    );
    let resolved = module(vec![identity], false);

    let resolved = psrs_desugar::desugar_module(resolved).unwrap();
    let typed = typecheck_module(resolved).unwrap();
    assert_eq!(typed.declarations[0].quantified.len(), 1);
    assert_eq!(
        typed.types[typed.declarations[0].ty.0 as usize],
        Type::Function {
            parameter: TypeId(0),
            result: TypeId(0),
        }
    );
    assert!(matches!(typed.types[0], Type::Variable(_)));
    typed.verify().unwrap();
}

#[test]
fn checks_a_declared_polymorphic_signature() {
    let signature = HirType {
        kind: HirTypeKind::Variable("a".into()),
        span: TextRange::new(20, 21),
    };
    let identity = declaration_with_signature(
        0,
        "identity",
        19,
        HirType {
            kind: HirTypeKind::Function {
                parameter: Box::new(signature.clone()),
                result: Box::new(signature),
            },
            span: TextRange::new(20, 26),
        },
        expr(
            HirExprKind::Lambda {
                binder: LocalBinder {
                    id: LocalId(0),
                    name: "x".into(),
                    span: TextRange::new(40, 41),
                },
                body: Box::new(local(0, 44)),
            },
            39,
            45,
        ),
    );
    let resolved = module(vec![identity], false);

    let resolved = psrs_desugar::desugar_module(resolved).unwrap();
    let typed = typecheck_module(resolved).unwrap();
    assert_eq!(typed.declarations[0].quantified.len(), 1);
    assert!(typed.types.iter().any(|ty| matches!(ty, Type::Variable(_))));
    typed.verify().unwrap();
}

#[test]
fn rejects_a_body_that_does_not_match_its_signature() {
    let signature = type_variable("a", 20);
    let declaration = declaration_with_signature(
        0,
        "bad",
        19,
        HirType {
            kind: HirTypeKind::Function {
                parameter: Box::new(signature.clone()),
                result: Box::new(signature),
            },
            span: TextRange::new(20, 26),
        },
        expr(
            HirExprKind::Lambda {
                binder: LocalBinder {
                    id: LocalId(0),
                    name: "x".into(),
                    span: TextRange::new(40, 41),
                },
                body: Box::new(integer("1", 44)),
            },
            39,
            45,
        ),
    );
    let resolved = module(vec![declaration], false);

    let errors = typecheck_module(resolved).unwrap_err();
    let mismatch = errors
        .iter()
        .find(|error| error.kind == TypeCheckErrorKind::TypeMismatch)
        .expect("expected a type mismatch");
    assert!(mismatch.message().contains("signature mismatch"));
}

#[test]
fn generalizes_let_bound_functions() {
    let let_expression = expr(
        HirExprKind::Let {
            bindings: vec![hir::LocalBinding {
                binder: LocalBinder {
                    id: LocalId(0),
                    name: "id".into(),
                    span: TextRange::new(30, 32),
                },
                value: expr(
                    HirExprKind::Lambda {
                        binder: LocalBinder {
                            id: LocalId(1),
                            name: "x".into(),
                            span: TextRange::new(34, 35),
                        },
                        body: Box::new(local(1, 38)),
                    },
                    33,
                    39,
                ),
                span: TextRange::new(26, 39),
            }],
            body: Box::new(expr(
                HirExprKind::Application(Box::new(local(0, 44)), Box::new(local(0, 46))),
                44,
                47,
            )),
        },
        26,
        47,
    );
    let resolved = module(vec![declaration(0, "main", 19, let_expression)], false);

    let resolved = psrs_desugar::desugar_module(resolved).unwrap();
    let typed = typecheck_module(resolved).unwrap();
    assert_eq!(typed.declarations[0].quantified.len(), 1);
    typed.verify().unwrap();
}

#[test]
fn rejects_infinite_types() {
    let self_application = module(
        vec![declaration(
            0,
            "self",
            19,
            expr(
                HirExprKind::Application(
                    Box::new(expr(
                        HirExprKind::Global(SymbolId::new(ModuleId(0), 0)),
                        26,
                        30,
                    )),
                    Box::new(expr(
                        HirExprKind::Global(SymbolId::new(ModuleId(0), 0)),
                        31,
                        35,
                    )),
                ),
                26,
                35,
            ),
        )],
        false,
    );
    let errors = typecheck_module(self_application).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::OccursCheck)
    );
}

#[test]
fn rejects_integer_literals_outside_i32() {
    let resolved = module(
        vec![declaration(0, "main", 19, integer("2147483648", 26))],
        false,
    );
    let errors = typecheck_module(resolved).unwrap_err();
    assert_eq!(errors[0].kind, TypeCheckErrorKind::IntegerOutOfRange);
}

mod user_types;
