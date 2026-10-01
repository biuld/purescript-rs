use super::*;
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};

const SPAN: TextRange = TextRange::new(0, 1);

fn arrow(types: &mut Vec<Type>, parameter: TypeId, result: TypeId) -> TypeId {
    let function = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Function));
    let applied = TypeId(types.len() as u32);
    types.push(Type::Application(function, parameter));
    let full = TypeId(types.len() as u32);
    types.push(Type::Application(applied, result));
    full
}

fn record(types: &mut Vec<Type>, fields: Vec<(&str, TypeId)>, tail: TypeId) -> TypeId {
    let mut row = tail;
    for (label, ty) in fields.into_iter().rev() {
        let id = TypeId(types.len() as u32);
        types.push(Type::RowExtend {
            label: label.into(),
            ty,
            tail: row,
        });
        row = id;
    }
    let head = TypeId(types.len() as u32);
    types.push(Type::Constructor(TypeConstructor::Record));
    let record = TypeId(types.len() as u32);
    types.push(Type::Application(head, row));
    record
}

fn declaration(
    index: u32,
    quantified: Vec<TypeVariableId>,
    ty: TypeId,
    value: Expr,
) -> Declaration {
    Declaration {
        symbol: SymbolId::new(ModuleId(0), index),
        name: format!("d{index}"),
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
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations,
        span: SPAN,
    }
}

fn rejected(module: &Module, message: &str) {
    let errors = module
        .verify()
        .expect_err("malformed THIR should be rejected");
    assert!(
        errors.iter().any(|error| error.message == message),
        "expected {message:?}; got {errors:?}"
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
    rejected(
        &module(
            types,
            vec![declaration(
                0,
                Vec::new(),
                quantified,
                Expr {
                    kind: ExprKind::Integer(1),
                    ty: TypeId(0),
                    span: SPAN,
                },
            )],
        ),
        "forall binders must be non-empty, unique, and lexically distinct",
    );
}

#[test]
fn verifier_rejects_a_forall_instantiated_at_two_different_types() {
    let variable = TypeVariableId(0);
    let mut types = vec![
        Type::Variable(variable),
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
    ];
    let scheme = arrow(&mut types, TypeId(0), TypeId(0));
    let instance = arrow(&mut types, TypeId(1), TypeId(2));
    let identity = declaration(
        1,
        vec![variable],
        scheme,
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: TypeId(0),
                    span: SPAN,
                },
                body: Box::new(Expr {
                    kind: ExprKind::Local(LocalId(0)),
                    ty: TypeId(0),
                    span: SPAN,
                }),
            },
            ty: scheme,
            span: SPAN,
        },
    );
    let main = declaration(
        0,
        Vec::new(),
        instance,
        Expr {
            kind: ExprKind::Global(SymbolId::new(ModuleId(0), 1)),
            ty: instance,
            span: SPAN,
        },
    );
    rejected(
        &module(types, vec![main, identity]),
        "global reference is not a valid scheme instance",
    );
}

#[test]
fn verifier_rejects_extra_fields_of_a_closed_record() {
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::RowEmpty,
    ];
    let closed = record(&mut types, vec![("a", TypeId(0))], TypeId(2));
    let wider = record(
        &mut types,
        vec![("a", TypeId(0)), ("b", TypeId(1))],
        TypeId(2),
    );
    let scheme = arrow(&mut types, closed, TypeId(0));
    let instance = arrow(&mut types, wider, TypeId(0));
    let definition = declaration(
        1,
        Vec::new(),
        scheme,
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: closed,
                    span: SPAN,
                },
                body: Box::new(Expr {
                    kind: ExprKind::Integer(0),
                    ty: TypeId(0),
                    span: SPAN,
                }),
            },
            ty: scheme,
            span: SPAN,
        },
    );
    let main = declaration(
        0,
        Vec::new(),
        instance,
        Expr {
            kind: ExprKind::Global(SymbolId::new(ModuleId(0), 1)),
            ty: instance,
            span: SPAN,
        },
    );
    rejected(
        &module(types, vec![main, definition]),
        "global reference is not a valid scheme instance",
    );
}

#[test]
fn verifier_accepts_instantiation_of_a_flexible_row_tail() {
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::Variable(TypeVariableId(0)),
        Type::RowEmpty,
    ];
    let open = record(&mut types, vec![("a", TypeId(0))], TypeId(2));
    let wider = record(
        &mut types,
        vec![("a", TypeId(0)), ("b", TypeId(1))],
        TypeId(3),
    );
    let scheme = arrow(&mut types, open, TypeId(0));
    let instance = arrow(&mut types, wider, TypeId(0));
    let definition = declaration(
        1,
        vec![TypeVariableId(0)],
        scheme,
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: open,
                    span: SPAN,
                },
                body: Box::new(Expr {
                    kind: ExprKind::Integer(0),
                    ty: TypeId(0),
                    span: SPAN,
                }),
            },
            ty: scheme,
            span: SPAN,
        },
    );
    let main = declaration(
        0,
        Vec::new(),
        instance,
        Expr {
            kind: ExprKind::Global(SymbolId::new(ModuleId(0), 1)),
            ty: instance,
            span: SPAN,
        },
    );
    assert!(
        module(types, vec![main, definition]).verify().is_ok(),
        "a quantified row tail can be instantiated to the residual fields"
    );
}

#[test]
fn verifier_rejects_extra_fields_of_a_rigid_open_row() {
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::Variable(TypeVariableId(0)),
        Type::RowEmpty,
    ];
    let open = record(&mut types, vec![("a", TypeId(0))], TypeId(2));
    let wider = record(
        &mut types,
        vec![("a", TypeId(0)), ("b", TypeId(1))],
        TypeId(3),
    );
    let function = arrow(&mut types, open, TypeId(0));
    let definition = declaration(
        1,
        vec![TypeVariableId(0)],
        function,
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: open,
                    span: SPAN,
                },
                body: Box::new(Expr {
                    kind: ExprKind::Integer(0),
                    ty: TypeId(0),
                    span: SPAN,
                }),
            },
            ty: function,
            span: SPAN,
        },
    );
    let argument = Expr {
        kind: ExprKind::Record(vec![
            (
                "a".into(),
                Expr {
                    kind: ExprKind::Integer(0),
                    ty: TypeId(0),
                    span: SPAN,
                },
            ),
            (
                "b".into(),
                Expr {
                    kind: ExprKind::Boolean(false),
                    ty: TypeId(1),
                    span: SPAN,
                },
            ),
        ]),
        ty: wider,
        span: SPAN,
    };
    let call = Expr {
        kind: ExprKind::Application(
            Box::new(Expr {
                kind: ExprKind::Global(SymbolId::new(ModuleId(0), 1)),
                ty: function,
                span: SPAN,
            }),
            Box::new(argument),
        ),
        ty: TypeId(0),
        span: SPAN,
    };
    let main = declaration(0, vec![TypeVariableId(0)], TypeId(0), call);
    rejected(
        &module(types, vec![main, definition]),
        "application argument or result type is inconsistent",
    );
}

#[test]
fn verifier_rejects_two_residuals_for_one_row_variable() {
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::Variable(TypeVariableId(0)),
        Type::RowEmpty,
    ];
    let parameter = record(&mut types, vec![("b", TypeId(0))], TypeId(2));
    let scheme = arrow(&mut types, parameter, parameter);
    let domain = record(
        &mut types,
        vec![("a", TypeId(1)), ("b", TypeId(0))],
        TypeId(3),
    );
    let codomain = record(
        &mut types,
        vec![("c", TypeId(1)), ("b", TypeId(0))],
        TypeId(3),
    );
    let instance = arrow(&mut types, domain, codomain);
    let definition = declaration(
        1,
        vec![TypeVariableId(0)],
        scheme,
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: parameter,
                    span: SPAN,
                },
                body: Box::new(Expr {
                    kind: ExprKind::Local(LocalId(0)),
                    ty: parameter,
                    span: SPAN,
                }),
            },
            ty: scheme,
            span: SPAN,
        },
    );
    let main = declaration(
        0,
        Vec::new(),
        instance,
        Expr {
            kind: ExprKind::Global(SymbolId::new(ModuleId(0), 1)),
            ty: instance,
            span: SPAN,
        },
    );
    rejected(
        &module(types, vec![main, definition]),
        "global reference is not a valid scheme instance",
    );
}
