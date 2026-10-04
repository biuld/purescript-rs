//! `list<record>` aggregate payloads.

use super::fixtures::{reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, ReprId, Representation, Signature, ValueDecl,
    ValueShape, VariantCase,
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

fn finish(
    wit: &str,
    external_symbol: SymbolId,
    main_symbol: SymbolId,
    result: ValueShape,
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
            projection: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result,
            }),
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
            source_module: ModuleId(0),
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
    finish(
        "package wasi:io@0.2.12; interface streams { record pair { x: s32, y: bool } get: func() -> option<list<pair>>; }",
        external_symbol,
        main_symbol,
        reference(0),
        vec![
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: Vec::new(),
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![reference(1)],
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

/// `result<list<pair>, s32>` -> `Either Int (Array Pair)`, the error on `Left`.
pub(super) fn result_list_record_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    finish(
        "package wasi:io@0.2.12; interface streams { record pair { x: s32, y: bool } get: func() -> result<list<pair>, s32>; }",
        external_symbol,
        main_symbol,
        reference(1),
        vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: vec![ValueShape::Integer],
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![reference(2)],
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
