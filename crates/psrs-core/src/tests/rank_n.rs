use super::*;
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};
use psrs_span::TextRange;

const SPAN: TextRange = TextRange::new(0, 1);

fn arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let function = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Function));
    let applied_parameter = TypeId(types.len() as u32);
    types.push(Type::Application(function, parameter));
    let applied_result = TypeId(types.len() as u32);
    types.push(Type::Application(applied_parameter, result));
    applied_result
}

fn declaration(
    index: u32,
    name: &str,
    quantified: Vec<TypeVariableId>,
    ty: TypeId,
    value: Expr,
) -> Declaration {
    Declaration {
        symbol: SymbolId::new(ModuleId(0), index),
        name: name.into(),
        name_span: SPAN,
        quantified,
        ty,
        value,
        span: SPAN,
    }
}

fn module(types: Vec<Type>, declarations: Vec<Declaration>) -> Module {
    Module {
        type_names: Vec::new(),
        id: ModuleId(0),
        name: "RankN".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations,
        entry: None,
        span: SPAN,
    }
}

fn global(index: u32, ty: TypeId) -> Expr {
    Expr {
        kind: ExprKind::Global(SymbolId::new(ModuleId(0), index)),
        ty,
        span: SPAN,
    }
}

fn assert_rejected_with(module: &Module, expected: &str) {
    let errors = module
        .verify()
        .expect_err("malformed Core should be rejected");
    assert!(
        errors.iter().any(|error| error.message == expected),
        "expected error {expected:?}; got {errors:?}"
    );
}

#[test]
fn verifier_rejects_an_inconsistent_instance_of_a_polymorphic_scheme() {
    let variable = TypeVariableId(0);
    let mut types = vec![
        Type::Variable(variable),
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
    ];
    let identity_type = arrow(&mut types, TypeId(0), TypeId(0));
    let forged_instance = arrow(&mut types, TypeId(1), TypeId(2));
    let identity = declaration(
        1,
        "identity",
        vec![variable],
        identity_type,
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(1),
                    name: "value".into(),
                    ty: TypeId(0),
                    span: SPAN,
                },
                body: Box::new(Expr {
                    kind: ExprKind::Local(LocalId(1)),
                    ty: TypeId(0),
                    span: SPAN,
                }),
            },
            ty: identity_type,
            span: SPAN,
        },
    );
    let main = declaration(
        0,
        "main",
        Vec::new(),
        forged_instance,
        global(1, forged_instance),
    );

    assert_rejected_with(
        &module(types, vec![main, identity]),
        "Core expression type is inconsistent with its context",
    );
}

#[test]
fn verifier_rejects_a_specialized_function_as_a_polymorphic_argument() {
    let identity_variable = TypeVariableId(0);
    let argument_variable = TypeVariableId(1);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Variable(identity_variable),
        Type::Variable(argument_variable),
    ];
    let identity_type = arrow(&mut types, TypeId(1), TypeId(1));
    let polymorphic_argument = arrow(&mut types, TypeId(2), TypeId(2));
    let polymorphic_argument = TypeId({
        let id = types.len() as u32;
        types.push(Type::ForAll {
            variables: vec![argument_variable],
            body: polymorphic_argument,
        });
        id
    });
    let consumer_type = arrow(&mut types, polymorphic_argument, TypeId(0));
    let specialized_identity = arrow(&mut types, TypeId(0), TypeId(0));

    let identity = declaration(
        1,
        "identity",
        vec![identity_variable],
        identity_type,
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(1),
                    name: "value".into(),
                    ty: TypeId(1),
                    span: SPAN,
                },
                body: Box::new(Expr {
                    kind: ExprKind::Local(LocalId(1)),
                    ty: TypeId(1),
                    span: SPAN,
                }),
            },
            ty: identity_type,
            span: SPAN,
        },
    );
    let consumer = declaration(
        2,
        "consumer",
        Vec::new(),
        consumer_type,
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(2),
                    name: "poly".into(),
                    ty: polymorphic_argument,
                    span: SPAN,
                },
                body: Box::new(Expr {
                    kind: ExprKind::Integer(0),
                    ty: TypeId(0),
                    span: SPAN,
                }),
            },
            ty: consumer_type,
            span: SPAN,
        },
    );
    let main = declaration(
        0,
        "main",
        Vec::new(),
        TypeId(0),
        Expr {
            kind: ExprKind::Application(
                Box::new(global(2, consumer_type)),
                Box::new(global(1, specialized_identity)),
            ),
            ty: TypeId(0),
            span: SPAN,
        },
    );

    assert_rejected_with(
        &module(types, vec![main, identity, consumer]),
        "Core expression type is inconsistent with its context",
    );
}

#[test]
fn verifier_rejects_duplicate_forall_binders() {
    let variable = TypeVariableId(0);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Variable(variable),
    ];
    let body = arrow(&mut types, TypeId(1), TypeId(1));
    let quantified_type = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![variable, variable],
        body,
    });
    let main = declaration(
        0,
        "main",
        Vec::new(),
        quantified_type,
        Expr {
            kind: ExprKind::Integer(0),
            ty: TypeId(0),
            span: SPAN,
        },
    );

    assert_rejected_with(
        &module(types, vec![main]),
        "forall binders must be non-empty, unique, and lexically distinct",
    );
}

#[test]
fn verifier_rejects_a_nested_forall_binder_that_shadows_its_outer_binder() {
    let variable = TypeVariableId(0);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Variable(variable),
    ];
    let body = arrow(&mut types, TypeId(1), TypeId(1));
    let inner = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![variable],
        body,
    });
    let outer = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: vec![variable],
        body: inner,
    });
    let main = declaration(
        0,
        "main",
        Vec::new(),
        outer,
        Expr {
            kind: ExprKind::Integer(0),
            ty: TypeId(0),
            span: SPAN,
        },
    );

    assert_rejected_with(
        &module(types, vec![main]),
        "forall binders must be non-empty, unique, and lexically distinct",
    );
}

#[test]
fn verifier_rejects_a_type_variable_outside_its_quantifier_scope() {
    let main = declaration(
        0,
        "main",
        Vec::new(),
        TypeId(0),
        Expr {
            kind: ExprKind::Integer(0),
            ty: TypeId(1),
            span: SPAN,
        },
    );

    assert_rejected_with(
        &module(
            vec![
                Type::Variable(TypeVariableId(0)),
                Type::Constructor(TypeConstructor::Int),
            ],
            vec![main],
        ),
        "type variable is outside its quantifier scope",
    );
}

#[test]
fn verifier_rejects_a_self_referential_forall_type_cycle() {
    let main = declaration(
        0,
        "main",
        Vec::new(),
        TypeId(0),
        Expr {
            kind: ExprKind::Integer(0),
            ty: TypeId(1),
            span: SPAN,
        },
    );

    assert_rejected_with(
        &module(
            vec![
                Type::ForAll {
                    variables: vec![TypeVariableId(0)],
                    body: TypeId(0),
                },
                Type::Constructor(TypeConstructor::Int),
            ],
            vec![main],
        ),
        "type table contains a cycle",
    );
}

#[test]
fn verifier_rejects_an_empty_forall() {
    let mut types = vec![Type::Constructor(TypeConstructor::Int)];
    let quantified = TypeId(types.len() as u32);
    types.push(Type::ForAll {
        variables: Vec::new(),
        body: TypeId(0),
    });
    let main = declaration(
        0,
        "main",
        Vec::new(),
        quantified,
        Expr {
            kind: ExprKind::Integer(0),
            ty: TypeId(0),
            span: SPAN,
        },
    );
    assert_rejected_with(
        &module(types, vec![main]),
        "forall binders must be non-empty, unique, and lexically distinct",
    );
}

#[test]
fn quantified_let_scopes_its_locals_and_rejects_unbound_local_variables() {
    for unbound in [false, true] {
        let outer = TypeVariableId(10);
        let inner = if unbound { TypeVariableId(11) } else { outer };
        let mut types = vec![Type::Variable(outer), Type::Variable(inner)];
        let outer_arrow = arrow(&mut types, TypeId(0), TypeId(0));
        let inner_arrow = arrow(&mut types, TypeId(1), TypeId(1));
        let quantified = TypeId(types.len() as u32);
        types.push(Type::ForAll {
            variables: vec![outer],
            body: outer_arrow,
        });
        let binder = Binder {
            id: LocalId(0),
            name: "ident".into(),
            ty: inner_arrow,
            span: SPAN,
        };
        let local_value = Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(1),
                    name: "x".into(),
                    ty: TypeId(1),
                    span: SPAN,
                },
                body: Box::new(Expr {
                    kind: ExprKind::Local(LocalId(1)),
                    ty: TypeId(1),
                    span: SPAN,
                }),
            },
            ty: inner_arrow,
            span: SPAN,
        };
        let value = Expr {
            kind: ExprKind::Let {
                bindings: vec![Binding {
                    binder,
                    quantified: Vec::new(),
                    value: local_value,
                    span: SPAN,
                }],
                body: Box::new(Expr {
                    kind: ExprKind::Local(LocalId(0)),
                    ty: outer_arrow,
                    span: SPAN,
                }),
            },
            ty: quantified,
            span: SPAN,
        };
        let module = module(
            types,
            vec![declaration(0, "main", Vec::new(), quantified, value)],
        );
        if unbound {
            assert!(
                module
                    .verify()
                    .unwrap_err()
                    .iter()
                    .any(|error| error.message == "type variable is outside its quantifier scope")
            );
        } else {
            module
                .verify()
                .expect("the enclosing forall scopes monomorphic local definitions");
        }
    }
}
