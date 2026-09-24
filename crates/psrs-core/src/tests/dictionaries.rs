use crate::*;
use psrs_hir::{LocalId, ModuleId, SymbolId};
use psrs_span::TextRange;
use psrs_thir as thir;

fn typed(kind: thir::ExprKind, ty: thir::TypeId, span: TextRange) -> thir::Expr {
    thir::Expr { kind, ty, span }
}

#[test]
fn lowering_erases_instance_and_superclass_evidence_to_calls_and_projections() {
    let module_id = ModuleId(0);
    let factory = SymbolId::new(module_id, 1);
    let eq_class = psrs_hir::TypeId::new(module_id, 0);
    let ord_class = psrs_hir::TypeId::new(module_id, 1);
    let dictionary = thir::TypeId(3);
    let parent_dictionary = thir::TypeId(4);
    let span = TextRange::new(0, 20);
    let module = thir::Module {
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            thir::Type::I32,
            thir::Type::Boolean,
            thir::Type::Function {
                parameter: thir::TypeId(0),
                result: thir::TypeId(1),
            },
            thir::Type::Record(vec![("isPositive".into(), thir::TypeId(2))]),
            thir::Type::Record(vec![
                ("rank".into(), thir::TypeId(0)),
                ("super".into(), dictionary),
            ]),
            thir::Type::Function {
                parameter: parent_dictionary,
                result: parent_dictionary,
            },
            thir::Type::Function {
                parameter: parent_dictionary,
                result: thir::TypeId(1),
            },
        ],
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            thir::Declaration {
                symbol: factory,
                name: "identityDictionary".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: thir::TypeId(5),
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
                    thir::TypeId(5),
                    span,
                ),
                span,
            },
            thir::Declaration {
                symbol: SymbolId::new(module_id, 0),
                name: "main".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: thir::TypeId(6),
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
                                                            constructor_type: thir::TypeId(5),
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
                                    thir::TypeId(2),
                                    span,
                                )),
                                Box::new(typed(thir::ExprKind::Integer(42), thir::TypeId(0), span)),
                            ),
                            thir::TypeId(1),
                            span,
                        )),
                    },
                    thir::TypeId(6),
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
    let ExprKind::FieldAccess { record, field } = &record.kind else {
        panic!("expected superclass evidence to be an ordinary field projection");
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
        id: module_id,
        name: "Main".into(),
        externals: Vec::new(),
        types: vec![
            thir::Type::I32,
            thir::Type::Record(vec![("value".into(), thir::TypeId(0))]),
        ],
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            thir::Declaration {
                symbol: dictionary_symbol,
                name: "numberDictionary".into(),
                name_span: span,
                quantified: Vec::new(),
                ty: thir::TypeId(1),
                value: typed(
                    thir::ExprKind::Record(vec![(
                        "value".into(),
                        typed(thir::ExprKind::Integer(7), thir::TypeId(0), span),
                    )]),
                    thir::TypeId(1),
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
                                ty: thir::TypeId(1),
                                span,
                            }),
                            thir::TypeId(1),
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
