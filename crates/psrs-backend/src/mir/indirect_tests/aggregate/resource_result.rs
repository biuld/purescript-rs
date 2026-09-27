//! A resource handle nested in a result aggregate, exposed without an automatic
//! drop (DEC-14).

use super::fixtures::{erased, reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, Representation, Signature, ValueDecl, ValueShape,
    VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

/// `result<own<file>, s32>` -> `Either Int Int`, where the owned handle is an
/// ordinary `Int` value the standard library drops explicitly.
pub(super) fn resource_result_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-resource.wit",
            "package wasi:io@0.2.12; interface streams { resource file; get: func() -> result<own<file>, s32>; }",
        )
        .expect("the resource result WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
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
        ],
        signatures: Vec::new(),
        product_labels: Default::default(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: reference(1),
        },
        ValueDecl {
            id: ValueId(1),
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
            destination: ValueId(1),
            kind: AssignmentKind::Constant(0),
            span: span(),
        },
    ];
    let module = cc::Module {
        name: "ResourceResultAbi".into(),
        externals: vec![External {
            result_guest: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: reference(1),
            }),
        }],
        representations,
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result: ValueId(1),
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
