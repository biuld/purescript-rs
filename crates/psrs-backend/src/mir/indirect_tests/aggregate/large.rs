//! An aggregate result whose canonical return area exceeds the scratch region.

use super::fixtures::{erased, reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, ExternalPayloads, PayloadField, PayloadNode,
    ReprId, Representation, Signature, ValueDecl, ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

/// `result<big, s32>` -> `Either Big Int` where
/// `Big = { a :: Number, b :: Number, c :: Int }`, a 24-byte payload whose
/// return area does not fit the 16-byte scratch region.
pub(super) fn large_record_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-large.wit",
            "package wasi:io@0.2.12; interface streams { record big { a: f64, b: f64, c: s64 } get: func() -> result<big, s32>; }",
        )
        .expect("the large record WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            Representation::Product {
                fields: vec![ValueShape::Number, ValueShape::Number, ValueShape::Integer],
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
        ],
        signatures: Vec::new(),
        product_labels: [(
            ReprId(1),
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        )]
        .into_iter()
        .collect(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: reference(2),
        },
        ValueDecl {
            id: ValueId(1),
            ty: reference(1),
        },
        ValueDecl {
            id: ValueId(2),
            ty: ValueShape::Integer,
        },
    ];
    let assignments = vec![
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
    ];
    let payloads = ExternalPayloads {
        parameters: Vec::new(),
        result: PayloadNode::Variant {
            representation: ReprId(2),
            cases: vec![
                Some(PayloadNode::Record {
                    representation: ReprId(1),
                    fields: vec![
                        PayloadField {
                            name: "a".into(),
                            node: PayloadNode::Value(ValueShape::Number),
                        },
                        PayloadField {
                            name: "b".into(),
                            node: PayloadNode::Value(ValueShape::Number),
                        },
                        PayloadField {
                            name: "c".into(),
                            node: PayloadNode::Value(ValueShape::Integer),
                        },
                    ],
                }),
                Some(PayloadNode::Value(ValueShape::Integer)),
            ],
        },
    };

    let module = cc::Module {
        name: "LargeRecordAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: reference(2),
            }),
            payloads,
        }],
        representations,
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
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
