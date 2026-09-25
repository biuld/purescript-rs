use super::*;
use crate::{Binder, Binding, Declaration, Expr, ExprKind, Module, Primitive, Type};
use psrs_hir::{LocalId, ModuleId, SymbolId};
use std::collections::HashMap;

#[test]
fn named_global_inlining_binds_arguments_once_before_effects_and_preserves_spans() {
    let int_type = TypeId(0);
    let function_type = TypeId(1);
    let function = SymbolId::new(ModuleId(0), 2);
    let argument = trace_call(function_type.0, int_type.0, 42, 25);
    let call_span = span(20, 33);
    let call = expression(
        ExprKind::Application(
            Box::new(expression(
                ExprKind::Global(function),
                function_type.0,
                20,
                23,
            )),
            Box::new(argument),
        ),
        int_type.0,
        call_span.start,
        call_span.end,
    );
    let value = expression(
        ExprKind::Let {
            bindings: vec![Binding {
                binder: Binder {
                    id: LocalId(0),
                    name: "outer".into(),
                    ty: int_type,
                    span: span(1, 6),
                },
                quantified: Vec::new(),
                value: expression(ExprKind::Integer(3), int_type.0, 8, 9),
                span: span(1, 9),
            }],
            body: Box::new(expression(
                ExprKind::Primitive {
                    op: Primitive::IntAdd,
                    left: Box::new(call),
                    right: Box::new(expression(ExprKind::Local(LocalId(0)), 0, 40, 45)),
                },
                int_type.0,
                20,
                45,
            )),
        },
        int_type.0,
        1,
        45,
    );
    let mut input = module(
        vec![
            Type::I32,
            Type::Function {
                parameter: int_type,
                result: int_type,
            },
        ],
        int_type.0,
        value,
    );
    input = with_trace(input);
    input.declarations.push(Declaration {
        symbol: function,
        name: "divideBeforeReturning".into(),
        name_span: span(50, 70),
        quantified: Vec::new(),
        ty: function_type,
        value: expression(
            ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: int_type,
                    span: span(51, 56),
                },
                body: Box::new(expression(
                    ExprKind::Let {
                        bindings: vec![Binding {
                            binder: Binder {
                                id: LocalId(1),
                                name: "trapping".into(),
                                ty: int_type,
                                span: span(60, 68),
                            },
                            quantified: Vec::new(),
                            value: expression(
                                ExprKind::Primitive {
                                    op: Primitive::IntDiv,
                                    left: Box::new(expression(ExprKind::Integer(1), 0, 70, 71)),
                                    right: Box::new(expression(ExprKind::Integer(0), 0, 72, 73)),
                                },
                                int_type.0,
                                70,
                                73,
                            ),
                            span: span(60, 73),
                        }],
                        body: Box::new(expression(ExprKind::Local(LocalId(0)), 0, 76, 81)),
                    },
                    int_type.0,
                    60,
                    81,
                )),
            },
            function_type.0,
            50,
            81,
        ),
        span: span(50, 81),
    });

    let original = run(&input);
    let optimized = optimize(input, Budget::default()).unwrap();
    let transformed = run(&optimized);
    assert!(original.0.is_err());
    assert!(transformed.0.is_err());
    assert_eq!(original.1, vec![42]);
    assert_eq!(transformed.1, original.1);
    let ExprKind::Let { bindings, body } = &optimized.declarations[0].value.kind else {
        panic!("the caller binding should remain in scope")
    };
    let ExprKind::Primitive {
        op: Primitive::IntAdd,
        left,
        ..
    } = &body.kind
    else {
        panic!("the inlined call should remain the left operand")
    };
    let ExprKind::Let {
        bindings: arguments,
        body: callee_body,
    } = &left.kind
    else {
        panic!("the call argument must be bound before the callee body")
    };
    assert_eq!(arguments.len(), 1);
    assert!(matches!(arguments[0].value.kind, ExprKind::Application(..)));
    assert_eq!(arguments[0].value.span, span(25, 32));
    assert_eq!(arguments[0].span, call_span);

    let ExprKind::Let {
        bindings: callee_bindings,
        body: result,
    } = &callee_body.kind
    else {
        panic!("the callee trap must remain after argument evaluation")
    };
    assert!(matches!(
        callee_bindings[0].value.kind,
        ExprKind::Primitive {
            op: Primitive::IntDiv,
            ..
        }
    ));
    assert_eq!(callee_bindings[0].value.span, span(70, 73));
    assert!(matches!(result.kind, ExprKind::Local(id) if id == arguments[0].binder.id));
    assert_ne!(bindings[0].binder.id, arguments[0].binder.id);
    assert_ne!(bindings[0].binder.id, callee_bindings[0].binder.id);
    assert_eq!(optimized.declarations[0].value.span, span(1, 45));
    optimized.verify().unwrap();
}

#[derive(Clone, Debug)]
enum Value {
    Integer(i32),
    Closure(Box<Closure>),
    External(SymbolId),
}

#[derive(Clone, Debug)]
struct Closure {
    binder: Binder,
    body: Expr,
    environment: HashMap<LocalId, Value>,
}

fn run(module: &Module) -> (Result<Value, ()>, Vec<i32>) {
    let mut trace = Vec::new();
    let result = evaluate(
        &module.declarations[0].value,
        module,
        &HashMap::new(),
        &mut trace,
    );
    (result, trace)
}

fn evaluate(
    expression: &Expr,
    module: &Module,
    environment: &HashMap<LocalId, Value>,
    trace: &mut Vec<i32>,
) -> Result<Value, ()> {
    match &expression.kind {
        ExprKind::Local(id) => environment.get(id).cloned().ok_or(()),
        ExprKind::Global(symbol) => {
            if module
                .externals
                .iter()
                .any(|external| external.symbol == *symbol)
            {
                return Ok(Value::External(*symbol));
            }
            let declaration = module
                .declarations
                .iter()
                .find(|declaration| declaration.symbol == *symbol)
                .ok_or(())?;
            evaluate(&declaration.value, module, &HashMap::new(), trace)
        }
        ExprKind::Integer(value) => Ok(Value::Integer(*value)),
        ExprKind::Lambda { binder, body } => Ok(Value::Closure(Box::new(Closure {
            binder: binder.clone(),
            body: (**body).clone(),
            environment: environment.clone(),
        }))),
        ExprKind::Application(function, argument) => {
            let function = evaluate(function, module, environment, trace)?;
            let argument = evaluate(argument, module, environment, trace)?;
            match function {
                Value::Closure(closure) => {
                    let mut environment = closure.environment;
                    environment.insert(closure.binder.id, argument);
                    evaluate(&closure.body, module, &environment, trace)
                }
                Value::External(symbol) => {
                    assert!(
                        module
                            .externals
                            .iter()
                            .any(|external| external.symbol == symbol)
                    );
                    if let Value::Integer(value) = argument {
                        trace.push(value);
                        Ok(Value::Integer(value))
                    } else {
                        Err(())
                    }
                }
                Value::Integer(_) => Err(()),
            }
        }
        ExprKind::Let { bindings, body } => {
            let mut environment = environment.clone();
            for binding in bindings {
                let value = evaluate(&binding.value, module, &environment, trace)?;
                environment.insert(binding.binder.id, value);
            }
            evaluate(body, module, &environment, trace)
        }
        ExprKind::Primitive { op, left, right } => {
            let Value::Integer(left) = evaluate(left, module, environment, trace)? else {
                return Err(());
            };
            let Value::Integer(right) = evaluate(right, module, environment, trace)? else {
                return Err(());
            };
            match op {
                Primitive::IntAdd => Ok(Value::Integer(left.wrapping_add(right))),
                Primitive::IntDiv if right != 0 => {
                    left.checked_div_euclid(right).map(Value::Integer).ok_or(())
                }
                Primitive::IntDiv => Err(()),
                _ => Err(()),
            }
        }
        _ => Err(()),
    }
}

#[test]
fn named_global_recursion_stays_as_a_call() {
    let int_type = TypeId(0);
    let function_type = TypeId(1);
    let function = SymbolId::new(ModuleId(0), 2);
    let call = expression(
        ExprKind::Application(
            Box::new(expression(
                ExprKind::Global(function),
                function_type.0,
                10,
                12,
            )),
            Box::new(expression(ExprKind::Integer(1), int_type.0, 13, 14)),
        ),
        int_type.0,
        10,
        14,
    );
    let mut input = module(
        vec![
            Type::I32,
            Type::Function {
                parameter: int_type,
                result: int_type,
            },
        ],
        int_type.0,
        call,
    );
    input.declarations.push(Declaration {
        symbol: function,
        name: "loop".into(),
        name_span: span(20, 24),
        quantified: Vec::new(),
        ty: function_type,
        value: expression(
            ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: int_type,
                    span: span(25, 30),
                },
                body: Box::new(expression(
                    ExprKind::Application(
                        Box::new(expression(
                            ExprKind::Global(function),
                            function_type.0,
                            31,
                            35,
                        )),
                        Box::new(expression(ExprKind::Local(LocalId(0)), 0, 36, 41)),
                    ),
                    int_type.0,
                    31,
                    41,
                )),
            },
            function_type.0,
            25,
            41,
        ),
        span: span(20, 41),
    });

    let optimized = optimize(input, Budget::default()).unwrap();
    assert!(matches!(
        &optimized.declarations[0].value.kind,
        ExprKind::Application(head, _) if matches!(&head.kind, ExprKind::Global(symbol) if *symbol == function)
    ));
}

#[test]
fn leaves_case_bodies_out_of_global_inlining_to_keep_diagnostics_singular() {
    let boolean_type = TypeId(0);
    let int_type = TypeId(1);
    let function_type = TypeId(2);
    let function = SymbolId::new(ModuleId(0), 2);
    let call = expression(
        ExprKind::Application(
            Box::new(expression(
                ExprKind::Global(function),
                function_type.0,
                10,
                15,
            )),
            Box::new(expression(ExprKind::Boolean(true), boolean_type.0, 16, 20)),
        ),
        int_type.0,
        10,
        20,
    );
    let mut input = module(
        vec![
            Type::Boolean,
            Type::I32,
            Type::Function {
                parameter: boolean_type,
                result: int_type,
            },
        ],
        int_type.0,
        call,
    );
    input.declarations.push(Declaration {
        symbol: function,
        name: "choose".into(),
        name_span: span(30, 36),
        quantified: Vec::new(),
        ty: function_type,
        value: expression(
            ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: boolean_type,
                    span: span(37, 42),
                },
                body: Box::new(expression(
                    ExprKind::Case {
                        scrutinee: Box::new(expression(
                            ExprKind::Local(LocalId(0)),
                            boolean_type.0,
                            45,
                            50,
                        )),
                        branches: vec![
                            CaseBranch {
                                pattern: Pattern {
                                    kind: PatternKind::Wildcard,
                                    ty: boolean_type,
                                    span: span(55, 56),
                                },
                                value: expression(ExprKind::Integer(1), int_type.0, 60, 61),
                                span: span(55, 61),
                            },
                            CaseBranch {
                                pattern: Pattern {
                                    kind: PatternKind::Wildcard,
                                    ty: boolean_type,
                                    span: span(65, 66),
                                },
                                value: expression(ExprKind::Integer(2), int_type.0, 70, 71),
                                span: span(65, 71),
                            },
                        ],
                    },
                    int_type.0,
                    45,
                    71,
                )),
            },
            function_type.0,
            37,
            71,
        ),
        span: span(30, 71),
    });

    let optimized = optimize(input, Budget::default()).unwrap();
    assert!(matches!(
        &optimized.declarations[0].value.kind,
        ExprKind::Application(head, _) if matches!(&head.kind, ExprKind::Global(symbol) if *symbol == function)
    ));
    optimized.verify().unwrap();
}
