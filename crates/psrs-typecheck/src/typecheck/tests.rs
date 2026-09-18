use super::*;
use psrs_hir::{
    Declaration as HirDeclaration, Expr as HirExpr, ExprKind as HirExprKind, ModuleId, SymbolId,
};
use psrs_resolve::bootstrap_intrinsics;

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
    }
}

fn module(declarations: Vec<HirDeclaration>, with_intrinsics: bool) -> hir::Module {
    hir::Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: if with_intrinsics {
            bootstrap_intrinsics()
        } else {
            Vec::new()
        },
        declarations,
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
fn rejects_unconstrained_and_infinite_types() {
    let unconstrained = module(
        vec![declaration(
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
                36,
            ),
        )],
        false,
    );
    let errors = typecheck_module(unconstrained).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.kind == TypeCheckErrorKind::UnconstrainedType)
    );

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
