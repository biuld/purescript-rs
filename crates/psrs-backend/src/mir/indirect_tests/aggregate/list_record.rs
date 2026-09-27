//! `list<record>` aggregate payloads.

use super::fixtures::{erased, reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, ExternalPayloads, PayloadField, PayloadNode,
    ReprId, Representation, Signature, ValueDecl, ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

fn pair() -> Representation {
    Representation::Product {
        fields: vec![ValueShape::Integer, ValueShape::Boolean],
    }
}

fn pair_node() -> PayloadNode {
    PayloadNode::Record {
        representation: ReprId(2),
        fields: vec![
            PayloadField {
                name: "x".into(),
                node: PayloadNode::Value(ValueShape::Integer),
            },
            PayloadField {
                name: "y".into(),
                node: PayloadNode::Value(ValueShape::Boolean),
            },
        ],
    }
}

fn finish(
    wit: &str,
    external_symbol: SymbolId,
    main_symbol: SymbolId,
    result: ValueShape,
    result_payload: PayloadNode,
    representations: Vec<Representation>,
    product_labels: Vec<(ReprId, Vec<String>)>,
) -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str("aggregate-list-record.wit", wit)
        .expect("the list record WIT fixture should resolve");

    let module = cc::Module {
        name: "ListRecordAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result,
            }),
            payloads: ExternalPayloads {
                parameters: Vec::new(),
                result: result_payload,
            },
        }],
        representations: cc::RepresentationTable {
            representations,
            signatures: Vec::new(),
            product_labels: product_labels.into_iter().collect(),
        },
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: result,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: ValueShape::String,
                },
                ValueDecl {
                    id: ValueId(2),
                    ty: ValueShape::Integer,
                },
            ],
            assignments: vec![
                Assignment {
                    destination: ValueId(0),
                    kind: AssignmentKind::DirectCall {
                        function: external_symbol,
                        arguments: Vec::new(),
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::Constant(0),
                    span: span(),
                },
            ],
            result: ValueId(2),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(main_symbol),
        span: span(),
    };
    let bindings = ExternalBindings {
        imports: vec![ExternalBinding {
            symbol: external_symbol,
            interface: "wasi:io/streams".into(),
            function: "get".into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}

/// `option<list<pair>>` -> `Maybe (Array Pair)`.
pub(super) fn option_list_record_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let list = PayloadNode::List {
        representation: ReprId(1),
        element: Box::new(pair_node()),
    };
    finish(
        "package wasi:io@0.2.12; interface streams { record pair { x: s32, y: bool } get: func() -> option<list<pair>>; }",
        external_symbol,
        main_symbol,
        reference(0),
        PayloadNode::Variant {
            representation: ReprId(0),
            cases: vec![None, Some(list)],
        },
        vec![
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: Vec::new(),
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![erased()],
                    },
                ],
            },
            Representation::Array {
                element: reference(2),
            },
            pair(),
        ],
        vec![(ReprId(2), vec!["x".to_string(), "y".to_string()])],
    )
}

/// `result<list<pair>, s32>` -> `Either (Array Pair) Int`.
pub(super) fn result_list_record_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let list = PayloadNode::List {
        representation: ReprId(2),
        element: Box::new(PayloadNode::Record {
            representation: ReprId(3),
            fields: pair_node_fields(),
        }),
    };
    finish(
        "package wasi:io@0.2.12; interface streams { record pair { x: s32, y: bool } get: func() -> result<list<pair>, s32>; }",
        external_symbol,
        main_symbol,
        reference(1),
        PayloadNode::Variant {
            representation: ReprId(1),
            cases: vec![Some(list), Some(PayloadNode::Value(ValueShape::Integer))],
        },
        vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: vec![erased()],
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![erased()],
                    },
                ],
            },
            Representation::Array {
                element: reference(3),
            },
            pair(),
        ],
        vec![(ReprId(3), vec!["x".to_string(), "y".to_string()])],
    )
}

fn pair_node_fields() -> Vec<PayloadField> {
    match pair_node() {
        PayloadNode::Record { fields, .. } => fields,
        _ => unreachable!("pair_node is a record"),
    }
}
