//! Synthesized fixed-length list fixtures. The source language cannot spell a
//! `list<T, N>` type, so these drive the inline canonical form directly.

use super::{reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, Representation, Signature, ValueDecl, ValueShape,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

/// `list<s32, 3>` -> `Array Int` as a parameter.
pub(crate) fn fixed_list_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "fixed-list.wit",
            "package wasi:io@0.2.12; interface streams { take: func(values: list<s32, 3>); }",
        )
        .expect("the fixed list WIT fixture should resolve");
    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(1),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(2),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(3),
            ty: reference(0),
        },
        ValueDecl {
            id: ValueId(4),
            ty: ValueShape::Integer,
        },
    ];
    let assignments = vec![
        Assignment {
            destination: ValueId(0),
            kind: AssignmentKind::Constant(1),
            span: span(),
        },
        Assignment {
            destination: ValueId(1),
            kind: AssignmentKind::Constant(2),
            span: span(),
        },
        Assignment {
            destination: ValueId(2),
            kind: AssignmentKind::Constant(3),
            span: span(),
        },
        Assignment {
            destination: ValueId(3),
            kind: AssignmentKind::ArrayNew {
                destination: ValueId(3),
                representation: cc::ReprId(0),
                elements: vec![ValueId(0), ValueId(1), ValueId(2)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(4),
            kind: AssignmentKind::DirectCall {
                function: external_symbol,
                arguments: vec![ValueId(3)],
            },
            span: span(),
        },
    ];
    let module = cc::Module {
        name: "FixedListAbi".into(),
        externals: vec![External {
            projection: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: vec![reference(0)],
                result: ValueShape::Integer,
            }),
        }],
        representations: cc::RepresentationTable {
            representations: vec![Representation::Array {
                element: ValueShape::Integer,
            }],
            signatures: Vec::new(),
            product_labels: Default::default(),
        },
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result: ValueId(4),
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
            function: "take".into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}

/// `list<s32, 3>` -> `Array Int` as a result.
pub(crate) fn fixed_list_result_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "fixed-list-result.wit",
            "package wasi:io@0.2.12; interface streams { get: func() -> list<s32, 3>; }",
        )
        .expect("the fixed list result WIT fixture should resolve");
    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let module = cc::Module {
        name: "FixedListResultAbi".into(),
        externals: vec![External {
            projection: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: reference(0),
            }),
        }],
        representations: cc::RepresentationTable {
            representations: vec![Representation::Array {
                element: ValueShape::Integer,
            }],
            signatures: Vec::new(),
            product_labels: Default::default(),
        },
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: reference(0),
                },
                ValueDecl {
                    id: ValueId(1),
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
                    destination: ValueId(1),
                    kind: AssignmentKind::Constant(0),
                    span: span(),
                },
            ],
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
