use super::*;

#[test]
fn creates_distinct_specializations_for_distinct_concrete_type_arguments() {
    let int_type = TypeId(2);
    let boolean_type = TypeId(3);
    let int_function_type = TypeId(4);
    let boolean_function_type = TypeId(5);
    let record_type = TypeId(6);
    let boolean_call = expression(
        ExprKind::Application(
            Box::new(expression(
                ExprKind::Global(SymbolId::new(ModuleId(0), 2)),
                boolean_function_type.0,
                20,
                23,
            )),
            Box::new(expression(ExprKind::Boolean(true), boolean_type.0, 24, 28)),
        ),
        boolean_type.0,
        20,
        28,
    );
    let value = expression(
        ExprKind::Let {
            bindings: vec![
                Binding {
                    binder: Binder {
                        id: LocalId(0),
                        name: "number".into(),
                        ty: int_type,
                        span: span(1, 7),
                    },
                    quantified: Vec::new(),
                    value: identity_call(int_function_type.0, int_type.0, 42, 10),
                    span: span(1, 15),
                },
                Binding {
                    binder: Binder {
                        id: LocalId(1),
                        name: "flag".into(),
                        ty: boolean_type,
                        span: span(16, 20),
                    },
                    quantified: Vec::new(),
                    value: boolean_call,
                    span: span(16, 28),
                },
            ],
            body: Box::new(expression(
                ExprKind::Record {
                    fields: vec![
                        (
                            "number".into(),
                            expression(ExprKind::Local(LocalId(0)), int_type.0, 30, 36),
                        ),
                        (
                            "flag".into(),
                            expression(ExprKind::Local(LocalId(1)), boolean_type.0, 38, 42),
                        ),
                    ],
                },
                record_type.0,
                30,
                42,
            )),
        },
        record_type.0,
        1,
        42,
    );
    let mut input = module(
        vec![
            Type::Variable(TypeVariableId(0)),
            Type::Function {
                parameter: TypeId(0),
                result: TypeId(0),
            },
            Type::I32,
            Type::Boolean,
            Type::Function {
                parameter: int_type,
                result: int_type,
            },
            Type::Function {
                parameter: boolean_type,
                result: boolean_type,
            },
            Type::Record(vec![
                ("number".into(), int_type),
                ("flag".into(), boolean_type),
            ]),
        ],
        record_type.0,
        value,
    );
    input
        .declarations
        .push(identity_declaration(1, TypeVariableId(0)));

    let optimized = optimize(input, preserve_specializations()).unwrap();
    let ExprKind::Let { bindings, .. } = &optimized.declarations[0].value.kind else {
        panic!("both instantiated calls should remain reachable")
    };
    let int_target = application_global(&bindings[0].value);
    let boolean_target = application_global(&bindings[1].value);
    assert_ne!(int_target, boolean_target);
    let specializations = optimized
        .declarations
        .iter()
        .filter(|declaration| declaration.name.starts_with("identity$p7_"))
        .collect::<Vec<_>>();
    assert_eq!(specializations.len(), 2);
    assert_eq!(
        optimized.types[specializations[0].ty.0 as usize],
        Type::Function {
            parameter: int_type,
            result: int_type,
        }
    );
    assert_eq!(
        optimized.types[specializations[1].ty.0 as usize],
        Type::Function {
            parameter: boolean_type,
            result: boolean_type,
        }
    );
    optimized.verify().unwrap();
}
