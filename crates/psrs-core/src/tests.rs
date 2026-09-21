use super::*;
use psrs_hir::{LocalId, ModuleId, SymbolId};

#[test]
fn verifier_rejects_out_of_range_types() {
    let module = Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![Type::I32],
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            name_span: TextRange::new(0, 4),
            quantified: Vec::new(),
            ty: TypeId(1),
            value: Expr {
                kind: ExprKind::Integer(0),
                ty: TypeId(0),
                span: TextRange::new(7, 8),
            },
            span: TextRange::new(0, 8),
        }],
        entry: None,
        span: TextRange::new(0, 8),
    };
    assert_eq!(
        module.verify().unwrap_err()[0].message,
        "type reference is outside the Core type table"
    );
}

#[test]
fn verifier_attributes_declaration_errors_to_their_source_module() {
    let owner = ModuleId(7);
    let module = Module {
        id: ModuleId(0),
        name: "Linked".into(),
        externals: Vec::new(),
        types: vec![Type::I32],
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(owner, 0),
            name: "broken".into(),
            name_span: TextRange::new(0, 6),
            quantified: Vec::new(),
            ty: TypeId(0),
            value: Expr {
                kind: ExprKind::Global(SymbolId::new(owner, 99)),
                ty: TypeId(0),
                span: TextRange::new(9, 15),
            },
            span: TextRange::new(0, 15),
        }],
        entry: None,
        span: TextRange::new(0, 15),
    };
    let error = module.verify().unwrap_err().remove(0);
    assert_eq!(error.module, owner);
    assert_eq!(error.message, "global reference is not declared");
}

fn single_declaration(types: Vec<Type>, declaration_type: TypeId, value: Expr) -> Module {
    Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![Declaration {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            name_span: TextRange::new(0, 4),
            quantified: Vec::new(),
            ty: declaration_type,
            value,
            span: TextRange::new(0, 20),
        }],
        entry: None,
        span: TextRange::new(0, 20),
    }
}

fn has_message(module: &Module, message: &str) -> bool {
    module
        .verify()
        .unwrap_err()
        .iter()
        .any(|error| error.message == message)
}

#[test]
fn verifier_rejects_a_declaration_body_with_the_wrong_type() {
    let module = single_declaration(
        vec![Type::I32, Type::Boolean],
        TypeId(0),
        Expr {
            kind: ExprKind::Boolean(true),
            ty: TypeId(1),
            span: TextRange::new(7, 11),
        },
    );
    assert!(has_message(
        &module,
        "Core expression type is inconsistent with its context"
    ));
}

#[test]
fn verifier_rejects_a_local_use_with_the_wrong_annotation() {
    let module = single_declaration(
        vec![
            Type::I32,
            Type::Boolean,
            Type::Function {
                parameter: TypeId(0),
                result: TypeId(1),
            },
        ],
        TypeId(2),
        Expr {
            kind: ExprKind::Lambda {
                binder: Binder {
                    id: LocalId(0),
                    name: "value".into(),
                    ty: TypeId(0),
                    span: TextRange::new(1, 6),
                },
                body: Box::new(Expr {
                    kind: ExprKind::Local(LocalId(0)),
                    ty: TypeId(1),
                    span: TextRange::new(9, 14),
                }),
            },
            ty: TypeId(2),
            span: TextRange::new(0, 14),
        },
    );
    assert!(has_message(
        &module,
        "Core expression type is inconsistent with its context"
    ));
}

#[test]
fn verifier_rejects_non_functions_and_wrong_application_arguments() {
    let non_function = single_declaration(
        vec![Type::I32, Type::Boolean],
        TypeId(0),
        Expr {
            kind: ExprKind::Application(
                Box::new(Expr {
                    kind: ExprKind::Integer(1),
                    ty: TypeId(0),
                    span: TextRange::new(0, 1),
                }),
                Box::new(Expr {
                    kind: ExprKind::Boolean(true),
                    ty: TypeId(1),
                    span: TextRange::new(2, 6),
                }),
            ),
            ty: TypeId(0),
            span: TextRange::new(0, 6),
        },
    );
    assert!(has_message(
        &non_function,
        "application target is not a function"
    ));

    let wrong_argument = single_declaration(
        vec![
            Type::I32,
            Type::Boolean,
            Type::Function {
                parameter: TypeId(0),
                result: TypeId(0),
            },
        ],
        TypeId(0),
        Expr {
            kind: ExprKind::Application(
                Box::new(Expr {
                    kind: ExprKind::Lambda {
                        binder: Binder {
                            id: LocalId(0),
                            name: "value".into(),
                            ty: TypeId(0),
                            span: TextRange::new(1, 6),
                        },
                        body: Box::new(Expr {
                            kind: ExprKind::Local(LocalId(0)),
                            ty: TypeId(0),
                            span: TextRange::new(9, 14),
                        }),
                    },
                    ty: TypeId(2),
                    span: TextRange::new(0, 14),
                }),
                Box::new(Expr {
                    kind: ExprKind::Boolean(true),
                    ty: TypeId(1),
                    span: TextRange::new(15, 19),
                }),
            ),
            ty: TypeId(0),
            span: TextRange::new(0, 19),
        },
    );
    assert!(has_message(
        &wrong_argument,
        "Core expression type is inconsistent with its context"
    ));
}

#[test]
fn verifier_rejects_invalid_if_constructor_and_array_types() {
    let invalid_if = single_declaration(
        vec![Type::I32, Type::Boolean],
        TypeId(0),
        Expr {
            kind: ExprKind::If {
                condition: Box::new(Expr {
                    kind: ExprKind::Integer(1),
                    ty: TypeId(0),
                    span: TextRange::new(0, 1),
                }),
                then_branch: Box::new(Expr {
                    kind: ExprKind::Integer(2),
                    ty: TypeId(0),
                    span: TextRange::new(2, 3),
                }),
                else_branch: Box::new(Expr {
                    kind: ExprKind::Boolean(false),
                    ty: TypeId(1),
                    span: TextRange::new(4, 9),
                }),
            },
            ty: TypeId(0),
            span: TextRange::new(0, 9),
        },
    );
    assert!(has_message(
        &invalid_if,
        "Core expression type is inconsistent with its context"
    ));

    let mut invalid_constructor = single_declaration(
        vec![Type::I32, Type::Boolean],
        TypeId(0),
        Expr {
            kind: ExprKind::Constructor {
                symbol: SymbolId::new(ModuleId(0), 1),
                arguments: vec![Expr {
                    kind: ExprKind::Boolean(true),
                    ty: TypeId(1),
                    span: TextRange::new(4, 8),
                }],
            },
            ty: TypeId(0),
            span: TextRange::new(0, 8),
        },
    );
    invalid_constructor.constructors.push(ConstructorInfo {
        symbol: SymbolId::new(ModuleId(0), 1),
        type_id: psrs_hir::TypeId::new(ModuleId(0), 0),
        tag: 0,
        field_count: 1,
        field_types: vec![TypeId(0)],
    });
    assert!(has_message(
        &invalid_constructor,
        "Core expression type is inconsistent with its context"
    ));

    let invalid_array = single_declaration(
        vec![
            Type::I32,
            Type::Boolean,
            Type::Constructor(TypeConstructor::Array),
            Type::Application(TypeId(2), TypeId(0)),
        ],
        TypeId(3),
        Expr {
            kind: ExprKind::Array {
                elements: vec![Expr {
                    kind: ExprKind::Boolean(true),
                    ty: TypeId(1),
                    span: TextRange::new(1, 5),
                }],
            },
            ty: TypeId(3),
            span: TextRange::new(0, 6),
        },
    );
    assert!(has_message(
        &invalid_array,
        "Core expression type is inconsistent with its context"
    ));
}
