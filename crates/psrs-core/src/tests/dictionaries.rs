use crate::*;
use psrs_hir::{LocalId, ModuleId, SymbolId};
use psrs_span::TextRange;
use psrs_thir as thir;

fn typed(kind: thir::ExprKind, ty: thir::TypeId, span: TextRange) -> thir::Expr {
    thir::Expr { kind, ty, span }
}

fn push_arrow(
    types: &mut Vec<thir::Type>,
    parameter: thir::TypeId,
    result: thir::TypeId,
) -> thir::TypeId {
    let head = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Constructor(thir::TypeConstructor::Function));
    let inner = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Application(head, parameter));
    let outer = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Application(inner, result));
    outer
}

fn push_record(types: &mut Vec<thir::Type>, fields: Vec<(&str, thir::TypeId)>) -> thir::TypeId {
    let mut sorted = fields;
    sorted.sort_by(|left, right| left.0.cmp(right.0));
    let row_empty = thir::TypeId(types.len() as u32);
    types.push(thir::Type::RowEmpty);
    let mut tail = row_empty;
    for (label, ty) in sorted.into_iter().rev() {
        let id = thir::TypeId(types.len() as u32);
        types.push(thir::Type::RowExtend {
            label: label.to_string(),
            ty,
            tail,
        });
        tail = id;
    }
    let head = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Constructor(thir::TypeConstructor::Record));
    let id = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Application(head, tail));
    id
}

#[test]
fn lowering_erases_instance_and_superclass_evidence_to_calls_and_projections() {
    let module_id = ModuleId(0);
    let factory = SymbolId::new(module_id, 1);
    let eq_class = psrs_hir::TypeId::new(module_id, 0);
    let ord_class = psrs_hir::TypeId::new(module_id, 1);
    let span = TextRange::new(0, 20);
    let mut types = vec![
        thir::Type::Constructor(thir::TypeConstructor::Int),
        thir::Type::Constructor(thir::TypeConstructor::Boolean),
    ];
    let i32_to_boolean = push_arrow(&mut types, thir::TypeId(0), thir::TypeId(1));
    let dictionary = push_record(&mut types, vec![("isPositive", i32_to_boolean)]);
    let unit = thir::TypeId(types.len() as u32);
    types.push(thir::Type::Constructor(thir::TypeConstructor::Unit));
    let superclass_thunk = push_arrow(&mut types, unit, dictionary);
    let parent_dictionary = push_record(
        &mut types,
        vec![("rank", thir::TypeId(0)), ("super", superclass_thunk)],
    );
    let identity_type = push_arrow(&mut types, parent_dictionary, parent_dictionary);
    let main_type = push_arrow(&mut types, parent_dictionary, thir::TypeId(1));
    let module = thir::Module {
        type_names: Vec::new(),
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types,
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            thir::Declaration {
                symbol: factory,
                name: "identityDictionary".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: identity_type,
                value: typed(
                    thir::ExprKind::Lambda {
                        binder: thir::Binder {
                            id: LocalId(1),
                            name: "dict".into(),
                            ty: parent_dictionary,
                            span,
                        },
                        body: Box::new(typed(
                            thir::ExprKind::Local(LocalId(1)),
                            parent_dictionary,
                            span,
                        )),
                    },
                    identity_type,
                    span,
                ),
                span,
            },
            thir::Declaration {
                symbol: SymbolId::new(module_id, 0),
                name: "main".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: main_type,
                value: typed(
                    thir::ExprKind::Lambda {
                        binder: thir::Binder {
                            id: LocalId(0),
                            name: "given".into(),
                            ty: parent_dictionary,
                            span,
                        },
                        body: Box::new(typed(
                            thir::ExprKind::Application(
                                Box::new(typed(
                                    thir::ExprKind::FieldAccess {
                                        expression: Box::new(typed(
                                            thir::ExprKind::Evidence(thir::Evidence {
                                                kind: thir::EvidenceKind::Superclass {
                                                    parent: Box::new(thir::Evidence {
                                                        kind: thir::EvidenceKind::Instance {
                                                            constructor: factory,
                                                            constructor_type: identity_type,
                                                            context: vec![thir::Evidence {
                                                                kind: thir::EvidenceKind::Given(
                                                                    LocalId(0),
                                                                ),
                                                                class_id: ord_class,
                                                                ty: parent_dictionary,
                                                                span,
                                                            }],
                                                        },
                                                        class_id: ord_class,
                                                        ty: parent_dictionary,
                                                        span,
                                                    }),
                                                    field: "super".into(),
                                                },
                                                class_id: eq_class,
                                                ty: dictionary,
                                                span,
                                            }),
                                            dictionary,
                                            span,
                                        )),
                                        field: "isPositive".into(),
                                    },
                                    i32_to_boolean,
                                    span,
                                )),
                                Box::new(typed(thir::ExprKind::Integer(42), thir::TypeId(0), span)),
                            ),
                            thir::TypeId(1),
                            span,
                        )),
                    },
                    main_type,
                    span,
                ),
                span,
            },
        ],
        span,
    };

    let core = lower_module(module).unwrap();
    core.verify().unwrap();
    let main = core
        .declarations
        .iter()
        .find(|declaration| declaration.name == "main")
        .unwrap();
    let ExprKind::Lambda { body, .. } = &main.value.kind else {
        panic!("expected the constrained binding to keep its evidence parameter");
    };
    let ExprKind::Application(method_call, _) = &body.kind else {
        panic!("expected the selected dictionary method to be called");
    };
    let ExprKind::FieldAccess { record, field } = &method_call.kind else {
        panic!("expected method selection to be an ordinary field projection");
    };
    assert_eq!(field, "isPositive");
    let ExprKind::Application(thunk, unit) = &record.kind else {
        panic!("expected superclass selection to force its thunk");
    };
    assert!(matches!(unit.kind, ExprKind::Unit));
    let ExprKind::FieldAccess { record, field } = &thunk.kind else {
        panic!("expected superclass selection to project its thunk");
    };
    assert_eq!(field, "super");
    assert!(matches!(record.kind, ExprKind::Application(_, _)));
}

#[test]
fn lowering_erases_global_dictionary_evidence_to_a_core_global() {
    let module_id = ModuleId(0);
    let dictionary_symbol = SymbolId::new(module_id, 1);
    let span = TextRange::new(0, 12);
    let module = thir::Module {
        type_names: Vec::new(),
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        external_types: Vec::new(),
        types: vec![
            thir::Type::Constructor(thir::TypeConstructor::Int),
            thir::Type::RowEmpty,
            thir::Type::RowExtend {
                label: "value".into(),
                ty: thir::TypeId(0),
                tail: thir::TypeId(1),
            },
            thir::Type::Constructor(thir::TypeConstructor::Record),
            thir::Type::Application(thir::TypeId(3), thir::TypeId(2)),
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        callable_types: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            thir::Declaration {
                symbol: dictionary_symbol,
                name: "numberDictionary".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: thir::TypeId(4),
                value: typed(
                    thir::ExprKind::Record(vec![(
                        "value".into(),
                        typed(thir::ExprKind::Integer(7), thir::TypeId(0), span),
                    )]),
                    thir::TypeId(4),
                    span,
                ),
                span,
            },
            thir::Declaration {
                symbol: SymbolId::new(module_id, 0),
                name: "main".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: thir::TypeId(0),
                value: typed(
                    thir::ExprKind::FieldAccess {
                        expression: Box::new(typed(
                            thir::ExprKind::Evidence(thir::Evidence {
                                kind: thir::EvidenceKind::Global(dictionary_symbol),
                                class_id: psrs_hir::TypeId::new(module_id, 0),
                                ty: thir::TypeId(4),
                                span,
                            }),
                            thir::TypeId(4),
                            span,
                        )),
                        field: "value".into(),
                    },
                    thir::TypeId(0),
                    span,
                ),
                span,
            },
        ],
        span,
    };

    let core = lower_module(module).unwrap();
    core.verify().unwrap();
    let main = core
        .declarations
        .iter()
        .find(|declaration| declaration.name == "main")
        .unwrap();
    let ExprKind::FieldAccess { record, field } = &main.value.kind else {
        panic!("expected global dictionary projection to be a Core field access");
    };
    assert_eq!(field, "value");
    assert!(matches!(record.kind, ExprKind::Global(symbol) if symbol == dictionary_symbol));
}
