use super::*;

#[test]
fn does_not_select_a_wildcard_after_an_unchecked_unknown_scrutinee() {
    let int_type = TypeId(0);
    let data_type = TypeId(1);
    let function_type = TypeId(2);
    let user_type = HirTypeId::new(ModuleId(0), 0);
    let constructor = SymbolId::new(ModuleId(0), 10);
    let value = expression(
        ExprKind::Lambda {
            binder: Binder {
                id: LocalId(0),
                name: "value".into(),
                ty: data_type,
                span: span(1, 6),
            },
            body: Box::new(expression(
                ExprKind::Case {
                    scrutinee: Box::new(expression(ExprKind::Local(LocalId(0)), 1, 10, 15)),
                    branches: vec![
                        CaseBranch {
                            pattern: Pattern {
                                kind: PatternKind::Constructor {
                                    symbol: constructor,
                                    arguments: Vec::new(),
                                },
                                ty: data_type,
                                span: span(20, 24),
                            },
                            value: expression(ExprKind::Integer(42), 0, 27, 29),
                            span: span(20, 29),
                        },
                        CaseBranch {
                            pattern: Pattern {
                                kind: PatternKind::Wildcard,
                                ty: data_type,
                                span: span(32, 33),
                            },
                            value: expression(ExprKind::Integer(0), 0, 36, 37),
                            span: span(32, 37),
                        },
                    ],
                },
                0,
                10,
                37,
            )),
        },
        function_type.0,
        1,
        37,
    );
    let mut module = module(
        vec![
            Type::I32,
            Type::Constructor(TypeConstructor::User(user_type)),
            Type::Function {
                parameter: data_type,
                result: int_type,
            },
        ],
        function_type.0,
        value,
    );
    module.constructors.push(crate::ConstructorInfo {
        symbol: constructor,
        name: "Only".into(),
        type_id: user_type,
        tag: 0,
        field_count: 0,
        field_types: Vec::new(),
    });
    let result = optimize(module, Budget::default()).unwrap();
    assert!(matches!(
        &result.declarations[0].value.kind,
        ExprKind::Lambda { body, .. } if matches!(&body.kind, ExprKind::Case { .. })
    ));
}

#[test]
fn out_of_range_array_index_remains_a_trapping_operation() {
    let int_type = TypeId(0);
    let array_type = TypeId(2);
    let value = expression(
        ExprKind::ArrayIndex {
            array: Box::new(expression(
                ExprKind::Array {
                    elements: vec![expression(ExprKind::Integer(1), 0, 5, 6)],
                },
                array_type.0,
                4,
                7,
            )),
            index: Box::new(expression(ExprKind::Integer(1), 0, 10, 11)),
        },
        int_type.0,
        4,
        11,
    );
    let result = optimize(
        module(
            vec![
                Type::I32,
                Type::Constructor(TypeConstructor::Array),
                Type::Application(TypeId(1), int_type),
            ],
            int_type.0,
            value,
        ),
        Budget::default(),
    )
    .unwrap();
    assert!(matches!(
        result.declarations[0].value.kind,
        ExprKind::ArrayIndex { .. }
    ));
}
