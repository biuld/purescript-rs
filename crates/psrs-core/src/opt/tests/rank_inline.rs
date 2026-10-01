use super::*;
use crate::{Binder, Binding, Declaration, ExprKind};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};

#[test]
fn global_inline_keeps_a_body_that_binds_its_own_type_variable() {
    let int_type = TypeId(0);
    let variable = TypeId(1);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Variable(TypeVariableId(0)),
    ];
    let identity = arrow_type(&mut types, variable, variable);
    let function_type = arrow_type(&mut types, int_type, int_type);
    let instantiated = function_type;
    let function = SymbolId::new(ModuleId(0), 2);
    let call = expression(
        ExprKind::Application(
            Box::new(expression(
                ExprKind::Global(function),
                function_type.0,
                10,
                11,
            )),
            Box::new(expression(ExprKind::Integer(1), int_type.0, 12, 13)),
        ),
        int_type.0,
        10,
        13,
    );
    let mut input = module(types, int_type.0, call);
    input.declarations[0].quantified = vec![TypeVariableId(0)];
    input.declarations.push(Declaration {
        symbol: function,
        name: "constant".into(),
        name_span: span(20, 28),
        quantified: Vec::new(),
        ty: function_type,
        value: expression(
            ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "ignored".into(),
                    ty: int_type,
                    span: span(29, 36),
                },
                body: Box::new(expression(
                    ExprKind::Let {
                        bindings: vec![Binding {
                            binder: Binder {
                                id: LocalId(1),
                                name: "identity".into(),
                                ty: identity,
                                span: span(40, 48),
                            },
                            quantified: vec![TypeVariableId(0)],
                            value: expression(
                                ExprKind::Lambda {
                                    binder: Binder {
                                        id: LocalId(2),
                                        name: "value".into(),
                                        ty: variable,
                                        span: span(49, 54),
                                    },
                                    body: Box::new(expression(
                                        ExprKind::Local(LocalId(2)),
                                        variable.0,
                                        55,
                                        60,
                                    )),
                                },
                                identity.0,
                                49,
                                60,
                            ),
                            span: span(40, 60),
                        }],
                        body: Box::new(expression(
                            ExprKind::Application(
                                Box::new(expression(
                                    ExprKind::Local(LocalId(1)),
                                    instantiated.0,
                                    61,
                                    69,
                                )),
                                Box::new(expression(ExprKind::Integer(1), int_type.0, 70, 71)),
                            ),
                            int_type.0,
                            61,
                            71,
                        )),
                    },
                    int_type.0,
                    40,
                    71,
                )),
            },
            function_type.0,
            29,
            62,
        ),
        span: span(20, 62),
    });

    let optimized = optimize(input, Budget::default()).expect("the call stays well scoped");
    assert!(
        matches!(
            &optimized.declarations[0].value.kind,
            ExprKind::Application(head, _)
                if matches!(&head.kind, ExprKind::Global(symbol) if *symbol == function)
        ),
        "inlining would copy the callee's type binder into the caller's scope"
    );
}

#[test]
fn global_inline_keeps_a_forall_signature_with_an_empty_quantified_list() {
    let variable = TypeId(0);
    let int_type = TypeId(1);
    let mut types = vec![
        Type::Variable(TypeVariableId(0)),
        Type::Constructor(TypeConstructor::Int),
    ];
    let identity = arrow_type(&mut types, variable, variable);
    let instantiated = arrow_type(&mut types, int_type, int_type);
    let quantified = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![TypeVariableId(0)],
        body: identity,
    });
    let function = SymbolId::new(ModuleId(0), 2);
    let call = expression(
        ExprKind::Application(
            Box::new(expression(
                ExprKind::Global(function),
                instantiated.0,
                10,
                11,
            )),
            Box::new(expression(ExprKind::Integer(1), int_type.0, 12, 13)),
        ),
        int_type.0,
        10,
        13,
    );
    let mut input = module(types, int_type.0, call);
    input.declarations.push(Declaration {
        symbol: function,
        name: "identity".into(),
        name_span: span(20, 28),
        quantified: Vec::new(),
        ty: quantified,
        value: expression(
            ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: variable,
                    span: span(29, 34),
                },
                body: Box::new(expression(ExprKind::Local(LocalId(0)), variable.0, 35, 40)),
            },
            quantified.0,
            29,
            40,
        ),
        span: span(20, 40),
    });

    let optimized =
        optimize(input, Budget::default()).expect("the polymorphic call stays in scope");
    assert!(matches!(
        &optimized.declarations[0].value.kind,
        ExprKind::Application(head, _)
            if matches!(&head.kind, ExprKind::Global(symbol) if *symbol == function)
    ));
}
