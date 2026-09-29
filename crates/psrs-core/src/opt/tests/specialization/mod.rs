mod instances;

use super::*;
use crate::{Binder, Binding, Declaration, ExprKind, Type, TypeConstructor};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};

fn identity_declaration(type_id: u32, variable: TypeVariableId) -> Declaration {
    identity_declaration_of(type_id, TypeId(0), variable)
}

fn identity_declaration_of(
    type_id: u32,
    argument_type: TypeId,
    variable: TypeVariableId,
) -> Declaration {
    Declaration {
        symbol: SymbolId::new(ModuleId(0), 2),
        name: "identity".into(),
        name_span: span(30, 38),
        quantified: vec![variable],
        ty: TypeId(type_id),
        value: expression(
            ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: argument_type,
                    span: span(39, 44),
                },
                body: Box::new(expression(
                    ExprKind::Local(LocalId(0)),
                    argument_type.0,
                    45,
                    50,
                )),
            },
            type_id,
            39,
            50,
        ),
        span: span(30, 50),
    }
}

fn identity_call(function_type: u32, value_type: u32, value: i32, start: u32) -> crate::Expr {
    expression(
        ExprKind::Application(
            Box::new(expression(
                ExprKind::Global(SymbolId::new(ModuleId(0), 2)),
                function_type,
                start,
                start + 3,
            )),
            Box::new(expression(
                ExprKind::Integer(value),
                value_type,
                start + 4,
                start + 5,
            )),
        ),
        value_type,
        start,
        start + 5,
    )
}

#[test]
fn specializes_concrete_calls_deduplicates_and_keeps_generic_fallback() {
    let mut types = vec![Type::Variable(TypeVariableId(0))];
    let generic_function = arrow_type(&mut types, TypeId(0), TypeId(0));
    let int_type = TypeId(types.len() as u32);
    types.push(Type::I32);
    let function_type = arrow_type(&mut types, int_type, int_type);
    let value = expression(
        ExprKind::Let {
            bindings: vec![Binding {
                binder: Binder {
                    id: LocalId(0),
                    name: "first".into(),
                    ty: int_type,
                    span: span(5, 10),
                },
                quantified: Vec::new(),
                value: identity_call(function_type.0, int_type.0, 1, 10),
                span: span(5, 15),
            }],
            body: Box::new(identity_call(function_type.0, int_type.0, 2, 20)),
        },
        int_type.0,
        5,
        25,
    );
    let mut input = module(types, int_type.0, value);
    input
        .declarations
        .push(identity_declaration(generic_function.0, TypeVariableId(0)));

    let optimized = optimize(input, preserve_specializations()).unwrap();
    let generic = optimized
        .declarations
        .iter()
        .find(|declaration| declaration.symbol == SymbolId::new(ModuleId(0), 2))
        .unwrap();
    assert_eq!(generic.quantified, vec![TypeVariableId(0)]);
    let specialized = optimized
        .declarations
        .iter()
        .filter(|declaration| declaration.name.starts_with("identity$p7_"))
        .collect::<Vec<_>>();
    assert_eq!(specialized.len(), 1);
    assert!(specialized[0].quantified.is_empty());
    let Some((parameter, result)) = crate::arrow_parts(&optimized.types, specialized[0].ty) else {
        panic!("the specialized declaration should have an arrow type")
    };
    assert_eq!(parameter, int_type);
    assert_eq!(result, int_type);
    let ExprKind::Lambda { binder, body } = &specialized[0].value.kind else {
        panic!("the specialized declaration should retain its body")
    };
    assert_eq!(binder.ty, int_type);
    assert_eq!(body.ty, int_type);
    assert_eq!(specialized[0].value.span, span(39, 50));

    let ExprKind::Let { bindings, body } = &optimized.declarations[0].value.kind else {
        panic!("both concrete calls should remain reachable")
    };
    let first_target = application_global(&bindings[0].value);
    let second_target = application_global(body);
    assert_eq!(first_target, specialized[0].symbol);
    assert_eq!(second_target, first_target);
    optimized.verify().unwrap();
}

#[test]
fn inlines_a_small_specialization_without_discarding_the_generic_declaration() {
    let mut types = vec![Type::Variable(TypeVariableId(0))];
    let generic_function = arrow_type(&mut types, TypeId(0), TypeId(0));
    let int_type = TypeId(types.len() as u32);
    types.push(Type::I32);
    let function_type = arrow_type(&mut types, int_type, int_type);
    let value = identity_call(function_type.0, int_type.0, 9, 10);
    let mut input = module(types, int_type.0, value);
    input
        .declarations
        .push(identity_declaration(generic_function.0, TypeVariableId(0)));

    let optimized = optimize(input, Budget::default()).unwrap();
    assert_eq!(optimized.declarations.len(), 2);
    assert!(matches!(
        optimized.declarations[0].value.kind,
        ExprKind::Let { .. }
    ));
    assert!(optimized.declarations.iter().any(|declaration| {
        declaration.symbol == SymbolId::new(ModuleId(0), 2)
            && declaration.quantified == vec![TypeVariableId(0)]
    }));
    optimized.verify().unwrap();
}

#[test]
fn does_not_specialize_a_call_that_still_has_a_type_variable() {
    let caller_variable = TypeVariableId(1);
    let mut types = vec![Type::Variable(TypeVariableId(0))];
    let generic_function = arrow_type(&mut types, TypeId(0), TypeId(0));
    let caller_variable_type = TypeId(types.len() as u32);
    types.push(Type::Variable(caller_variable));
    let caller_type = arrow_type(&mut types, caller_variable_type, caller_variable_type);
    let main = expression(
        ExprKind::Lambda {
            binder: Binder {
                id: LocalId(1),
                name: "input".into(),
                ty: caller_variable_type,
                span: span(3, 8),
            },
            body: Box::new(expression(
                ExprKind::Application(
                    Box::new(expression(
                        ExprKind::Global(SymbolId::new(ModuleId(0), 2)),
                        caller_type.0,
                        10,
                        15,
                    )),
                    Box::new(expression(
                        ExprKind::Local(LocalId(1)),
                        caller_variable_type.0,
                        16,
                        21,
                    )),
                ),
                caller_variable_type.0,
                10,
                21,
            )),
        },
        caller_type.0,
        3,
        21,
    );
    let mut input = module(types, caller_type.0, main);
    input.declarations[0].quantified.push(caller_variable);
    input
        .declarations
        .push(identity_declaration(generic_function.0, TypeVariableId(0)));

    let optimized = optimize(input, Budget::default()).unwrap();
    assert_eq!(optimized.declarations.len(), 2);
    let ExprKind::Lambda { body, .. } = &optimized.declarations[0].value.kind else {
        panic!("the polymorphic caller remains a function")
    };
    assert_eq!(application_global(body), SymbolId::new(ModuleId(0), 2));
    optimized.verify().unwrap();
}

#[test]
fn keeps_cross_module_calls_on_the_generic_declaration() {
    let mut types = vec![Type::Variable(TypeVariableId(0))];
    let generic_function = arrow_type(&mut types, TypeId(0), TypeId(0));
    let int_type = TypeId(types.len() as u32);
    types.push(Type::I32);
    let function_type = arrow_type(&mut types, int_type, int_type);
    let mut value = identity_call(function_type.0, int_type.0, 9, 10);
    let ExprKind::Application(function, _) = &mut value.kind else {
        panic!("expected an applied generic function")
    };
    let ExprKind::Global(symbol) = &mut function.kind else {
        panic!("expected a direct generic function reference")
    };
    *symbol = SymbolId::new(ModuleId(1), 0);

    let mut input = module(types, int_type.0, value);
    let mut generic = identity_declaration(generic_function.0, TypeVariableId(0));
    generic.symbol = SymbolId::new(ModuleId(1), 0);
    input.declarations.push(generic);

    let optimized = optimize(input, preserve_specializations()).unwrap();
    assert_eq!(optimized.declarations.len(), 2);
    assert_eq!(
        application_global(&optimized.declarations[0].value),
        SymbolId::new(ModuleId(1), 0)
    );
    optimized.verify().unwrap();
}

#[test]
fn specializes_nested_concrete_types_and_respects_budgets() {
    let mut types = vec![Type::Variable(TypeVariableId(0))];
    let array_constructor = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Array));
    let array_of_variable = TypeId(types.len() as u32);
    types.push(Type::Application(array_constructor, TypeId(0)));
    let generic_function = arrow_type(&mut types, array_of_variable, array_of_variable);
    let int_type = TypeId(types.len() as u32);
    types.push(Type::I32);
    let array_of_int = TypeId(types.len() as u32);
    types.push(Type::Application(array_constructor, int_type));
    let function_type = arrow_type(&mut types, array_of_int, array_of_int);
    let array_value = expression(
        ExprKind::Array {
            elements: vec![expression(ExprKind::Integer(7), int_type.0, 10, 11)],
        },
        array_of_int.0,
        9,
        12,
    );
    let value = expression(
        ExprKind::Application(
            Box::new(expression(
                ExprKind::Global(SymbolId::new(ModuleId(0), 2)),
                function_type.0,
                5,
                8,
            )),
            Box::new(array_value),
        ),
        array_of_int.0,
        5,
        12,
    );
    let mut input = module(types, array_of_int.0, value);
    input.declarations.push(identity_declaration_of(
        generic_function.0,
        array_of_variable,
        TypeVariableId(0),
    ));

    let optimized = optimize(input.clone(), preserve_specializations()).unwrap();
    let specialized = optimized
        .declarations
        .iter()
        .find(|declaration| declaration.name.starts_with("identity$p7_"))
        .unwrap();
    let Some((parameter, result)) = crate::arrow_parts(&optimized.types, specialized.ty) else {
        panic!("specialized function type should remain an arrow")
    };
    assert_eq!(parameter, array_of_int);
    assert_eq!(result, array_of_int);
    assert!(matches!(
        optimized.types[parameter.0 as usize],
        Type::Application(_, element) if element == int_type
    ));

    let no_count_budget = Budget {
        max_specializations: 0,
        ..Budget::default()
    };
    let count_limited = optimize(input.clone(), no_count_budget).unwrap();
    assert_eq!(count_limited.declarations.len(), 2);

    let no_size_budget = Budget {
        max_specialized_nodes: 1,
        ..Budget::default()
    };
    let size_limited = optimize(input, no_size_budget).unwrap();
    assert_eq!(size_limited.declarations.len(), 2);
}

fn application_global(expression: &crate::Expr) -> SymbolId {
    let mut head = expression;
    while let ExprKind::Application(function, _) = &head.kind {
        head = function;
    }
    match head.kind {
        ExprKind::Global(symbol) => symbol,
        _ => panic!("expected a direct global call"),
    }
}

fn preserve_specializations() -> Budget {
    Budget {
        max_inline_nodes: 1,
        ..Budget::default()
    }
}
