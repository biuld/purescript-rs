use super::*;
use crate::{
    Binder, CaseBranch, Declaration, Expr, ExprKind, Pattern, PatternKind, Primitive, Type,
    TypeConstructor, TypeId,
};
use psrs_hir::{ExternalKind, ExternalSymbol, LocalId, ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;

fn span(start: u32, end: u32) -> TextRange {
    TextRange::new(start, end)
}

fn expression(kind: ExprKind, ty: u32, start: u32, end: u32) -> Expr {
    Expr {
        kind,
        ty: TypeId(ty),
        span: span(start, end),
    }
}

fn module(types: Vec<Type>, declaration_type: u32, value: Expr) -> Module {
    Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(ModuleId(0), 1),
            name: "main".into(),
            name_span: span(0, 4),
            quantified: Vec::new(),
            ty: TypeId(declaration_type),
            value,
            span: span(0, 100),
        }],
        entry: None,
        span: span(0, 100),
    }
}

fn with_trace(mut module: Module) -> Module {
    module.externals.push(ExternalSymbol {
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "trace".into(),
        kind: ExternalKind::Wit {
            interface: "test:trace".into(),
            function: "trace".into(),
        },
        signature: None,
    });
    module
}

fn trace_call(function_type: u32, int_type: u32, argument: i32, start: u32) -> Expr {
    expression(
        ExprKind::Application(
            Box::new(expression(
                ExprKind::Global(SymbolId::new(ModuleId(0), 0)),
                function_type,
                start,
                start + 5,
            )),
            Box::new(expression(
                ExprKind::Integer(argument),
                int_type,
                start + 6,
                start + 7,
            )),
        ),
        int_type,
        start,
        start + 7,
    )
}

#[test]
fn folds_wrapping_integer_arithmetic_and_keeps_the_operation_span() {
    let value = expression(
        ExprKind::Primitive {
            op: Primitive::IntAdd,
            left: Box::new(expression(ExprKind::Integer(i32::MAX), 0, 5, 6)),
            right: Box::new(expression(ExprKind::Integer(1), 0, 9, 10)),
        },
        0,
        5,
        10,
    );
    let result = optimize(module(vec![Type::I32], 0, value), Budget::default()).unwrap();
    assert_eq!(
        result.declarations[0].value.kind,
        ExprKind::Integer(i32::MIN)
    );
    assert_eq!(result.declarations[0].value.span, span(5, 10));
}

#[test]
fn leaves_constant_division_that_would_trap() {
    let value = expression(
        ExprKind::Primitive {
            op: Primitive::IntQuot,
            left: Box::new(expression(ExprKind::Integer(1), 0, 5, 6)),
            right: Box::new(expression(ExprKind::Integer(0), 0, 9, 10)),
        },
        0,
        5,
        10,
    );
    let result = optimize(module(vec![Type::I32], 0, value), Budget::default()).unwrap();
    assert!(matches!(
        result.declarations[0].value.kind,
        ExprKind::Primitive {
            op: Primitive::IntQuot,
            ..
        }
    ));
}

#[test]
fn retains_an_unused_euclidean_division_that_may_trap() {
    let int_type = TypeId(0);
    let binding = crate::Binding {
        binder: Binder {
            id: LocalId(0),
            name: "unused".into(),
            ty: int_type,
            span: span(4, 10),
        },
        quantified: Vec::new(),
        value: expression(
            ExprKind::Primitive {
                op: Primitive::IntDiv,
                left: Box::new(expression(ExprKind::Integer(1), 0, 13, 14)),
                right: Box::new(expression(ExprKind::Integer(0), 0, 17, 18)),
            },
            0,
            13,
            18,
        ),
        span: span(4, 18),
    };
    let value = expression(
        ExprKind::Let {
            bindings: vec![binding],
            body: Box::new(expression(ExprKind::Integer(7), 0, 22, 23)),
        },
        0,
        4,
        23,
    );
    let result = optimize(module(vec![Type::I32], 0, value), Budget::default()).unwrap();
    let ExprKind::Let { bindings, .. } = &result.declarations[0].value.kind else {
        panic!("division by zero must remain observable even when unused")
    };
    assert!(matches!(
        bindings[0].value.kind,
        ExprKind::Primitive {
            op: Primitive::IntDiv,
            ..
        }
    ));
}

#[test]
fn algebraic_zero_does_not_remove_an_effectful_operand() {
    let int_type = TypeId(0);
    let function_type = TypeId(1);
    let value = expression(
        ExprKind::Primitive {
            op: Primitive::IntMul,
            left: Box::new(trace_call(function_type.0, int_type.0, 4, 5)),
            right: Box::new(expression(ExprKind::Integer(0), int_type.0, 14, 15)),
        },
        int_type.0,
        5,
        15,
    );
    let result = optimize(
        with_trace(module(
            vec![
                Type::I32,
                Type::Function {
                    parameter: int_type,
                    result: int_type,
                },
            ],
            int_type.0,
            value,
        )),
        Budget::default(),
    )
    .unwrap();
    assert!(matches!(
        result.declarations[0].value.kind,
        ExprKind::Primitive {
            op: Primitive::IntMul,
            ..
        }
    ));
}

#[test]
fn removes_only_inert_unused_bindings() {
    let int_type = TypeId(0);
    let function_type = TypeId(1);
    let types = vec![
        Type::I32,
        Type::Function {
            parameter: int_type,
            result: int_type,
        },
    ];
    let unused_inert = crate::Binding {
        binder: Binder {
            id: LocalId(0),
            name: "unused".into(),
            ty: int_type,
            span: span(4, 10),
        },
        quantified: Vec::new(),
        value: expression(ExprKind::Integer(1), 0, 13, 14),
        span: span(4, 14),
    };
    let effectful = crate::Binding {
        binder: Binder {
            id: LocalId(1),
            name: "observed".into(),
            ty: int_type,
            span: span(16, 24),
        },
        quantified: Vec::new(),
        value: trace_call(function_type.0, int_type.0, 2, 27),
        span: span(16, 34),
    };
    let value = expression(
        ExprKind::Let {
            bindings: vec![unused_inert, effectful],
            body: Box::new(expression(ExprKind::Integer(3), 0, 38, 39)),
        },
        0,
        0,
        39,
    );
    let result = optimize(with_trace(module(types, 0, value)), Budget::default()).unwrap();
    let ExprKind::Let { bindings, .. } = &result.declarations[0].value.kind else {
        panic!("the observable call should remain sequenced")
    };
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].binder.id, LocalId(1));
}

#[test]
fn beta_reduction_binds_an_effectful_argument_once_and_before_the_body() {
    let int_type = TypeId(0);
    let function_type = TypeId(1);
    let types = vec![
        Type::I32,
        Type::Function {
            parameter: int_type,
            result: int_type,
        },
    ];
    let lambda = expression(
        ExprKind::Lambda {
            binder: Binder {
                id: LocalId(3),
                name: "argument".into(),
                ty: int_type,
                span: span(4, 12),
            },
            body: Box::new(expression(ExprKind::Integer(9), 0, 16, 17)),
        },
        function_type.0,
        3,
        18,
    );
    let value = expression(
        ExprKind::Application(
            Box::new(lambda),
            Box::new(trace_call(function_type.0, int_type.0, 4, 20)),
        ),
        int_type.0,
        3,
        27,
    );
    let result = optimize(
        with_trace(module(types, int_type.0, value)),
        Budget::default(),
    )
    .unwrap();
    let ExprKind::Let { bindings, body } = &result.declarations[0].value.kind else {
        panic!("beta reduction must preserve the argument evaluation")
    };
    assert_eq!(bindings.len(), 1);
    assert!(matches!(bindings[0].value.kind, ExprKind::Application(..)));
    assert_eq!(body.kind, ExprKind::Integer(9));
    assert_eq!(result.declarations[0].value.span, span(3, 27));
}

#[test]
fn projection_from_a_known_record_preserves_field_evaluation_order() {
    let int_type = TypeId(0);
    let record_type = TypeId(1);
    let function_type = TypeId(2);
    let types = vec![
        Type::I32,
        Type::Record(vec![
            ("first".into(), int_type),
            ("second".into(), int_type),
        ]),
        Type::Function {
            parameter: int_type,
            result: int_type,
        },
    ];
    let value = expression(
        ExprKind::FieldAccess {
            record: Box::new(expression(
                ExprKind::Record {
                    fields: vec![
                        ("first".into(), trace_call(function_type.0, 0, 1, 5)),
                        ("second".into(), trace_call(function_type.0, 0, 2, 20)),
                    ],
                },
                record_type.0,
                4,
                28,
            )),
            field: "second".into(),
        },
        int_type.0,
        4,
        34,
    );
    let result = optimize(
        with_trace(module(types, int_type.0, value)),
        Budget::default(),
    )
    .unwrap();
    let ExprKind::Let { bindings, .. } = &result.declarations[0].value.kind else {
        panic!("record projection should become ordered bindings")
    };
    assert_eq!(bindings.len(), 2);
    assert!(
        bindings
            .iter()
            .all(|binding| matches!(binding.value.kind, ExprKind::Application(..)))
    );
    assert_eq!(bindings[0].value.span.start, 5);
    assert_eq!(bindings[1].value.span.start, 20);
}

#[test]
fn selects_a_known_constructor_case_and_substitutes_its_field() {
    let int_type = TypeId(0);
    let option_type = TypeId(1);
    let user_type = HirTypeId::new(ModuleId(0), 0);
    let constructor = SymbolId::new(ModuleId(0), 10);
    let value = expression(
        ExprKind::Case {
            scrutinee: Box::new(expression(
                ExprKind::Constructor {
                    symbol: constructor,
                    arguments: vec![expression(ExprKind::Integer(42), 0, 5, 7)],
                },
                option_type.0,
                4,
                8,
            )),
            branches: vec![CaseBranch {
                pattern: Pattern {
                    kind: PatternKind::Constructor {
                        symbol: constructor,
                        arguments: vec![Pattern {
                            kind: PatternKind::Var {
                                id: LocalId(0),
                                ty: int_type,
                            },
                            ty: int_type,
                            span: span(12, 13),
                        }],
                    },
                    ty: option_type,
                    span: span(11, 14),
                },
                value: expression(ExprKind::Local(LocalId(0)), int_type.0, 18, 19),
                span: span(11, 19),
            }],
        },
        int_type.0,
        4,
        19,
    );
    let mut module = module(
        vec![
            Type::I32,
            Type::Constructor(TypeConstructor::User(user_type)),
        ],
        int_type.0,
        value,
    );
    module.constructors.push(crate::ConstructorInfo {
        symbol: constructor,
        name: "Some".into(),
        type_id: user_type,
        tag: 0,
        field_count: 1,
        field_types: vec![int_type],
    });
    let result = optimize(module, Budget::default()).unwrap();
    let ExprKind::Let { bindings, body } = &result.declarations[0].value.kind else {
        panic!("case field binding should be shared with a fresh local")
    };
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].value.kind, ExprKind::Integer(42));
    assert_eq!(body.kind, ExprKind::Local(bindings[0].binder.id));
}

mod edge;
mod global_inline;
mod specialization;
