//! Flags and non-byte list aggregate fixtures.

use super::fixtures::{erased, reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, ExternalPayloads, PayloadNode, ReprId,
    Representation, Signature, ValueDecl, ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

/// Builds a module whose only external returns `option<inner>`: repr 0 is the
/// `Maybe` variant, repr 1 is the inner payload.
fn base(
    wit: &str,
    function: &str,
    inner: Representation,
) -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str("aggregate-collection.wit", wit)
        .expect("the collection WIT fixture should resolve");

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: reference(0),
        },
        ValueDecl {
            id: ValueId(1),
            ty: ValueShape::String,
        },
        ValueDecl {
            id: ValueId(2),
            ty: reference(1),
        },
        ValueDecl {
            id: ValueId(3),
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
            destination: ValueId(3),
            kind: AssignmentKind::Constant(0),
            span: span(),
        },
    ];
    let module = cc::Module {
        name: "CollectionAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: reference(0),
            }),
            payloads: ExternalPayloads {
                parameters: Vec::new(),
                result: PayloadNode::Variant {
                    representation: ReprId(0),
                    cases: vec![None, Some(PayloadNode::Value(reference(1)))],
                },
            },
        }],
        representations: cc::RepresentationTable {
            representations: vec![
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
                inner,
            ],
            signatures: Vec::new(),
            product_labels: Default::default(),
        },
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result: ValueId(3),
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
            function: function.into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}

/// `option<access>` -> `Maybe Access` where `access` is a WIT flags type.
pub(super) fn flags_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let (mut module, bindings, resolve) = base(
        "package wasi:io@0.2.12; interface streams { flags access { read, write } get: func() -> option<access>; }",
        "get",
        Representation::Product {
            fields: vec![ValueShape::Boolean, ValueShape::Boolean],
        },
    );
    module.representations.product_labels =
        [(ReprId(1), vec!["read".to_string(), "write".to_string()])]
            .into_iter()
            .collect();
    (module, bindings, resolve)
}

/// `option<list<s32>>` -> `Maybe (Array Int)`.
pub(super) fn non_byte_list_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    base(
        "package wasi:io@0.2.12; interface streams { get: func() -> option<list<s32>>; }",
        "get",
        Representation::Array {
            element: ValueShape::Integer,
        },
    )
}
