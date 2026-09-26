use super::{reference, span};
use crate::cc::{self, Assignment, AssignmentKind, External, Signature, ValueDecl, ValueShape};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

pub(crate) fn flags_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "flags-list.wit",
            "package wasi:io@0.2.12; interface streams { flags access { write, read } take: func(values: list<access>); }",
        )
        .expect("the flags list WIT fixture should resolve");

    // Source flag fields are sorted alphabetically: `read` then `write`. WIT
    // declares `write` before `read`, so `write` is bit 0 and `read` is bit 1.
    let representations = cc::RepresentationTable {
        representations: vec![
            cc::Representation::Product {
                fields: vec![ValueShape::Boolean, ValueShape::Boolean],
            },
            cc::Representation::Array {
                element: reference(0),
            },
        ],
        signatures: Vec::new(),
        product_labels: [(cc::ReprId(0), vec!["read".to_string(), "write".to_string()])]
            .into_iter()
            .collect(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueShape::Boolean,
        },
        ValueDecl {
            id: ValueId(1),
            ty: ValueShape::Boolean,
        },
        ValueDecl {
            id: ValueId(2),
            ty: reference(0),
        },
        ValueDecl {
            id: ValueId(3),
            ty: reference(1),
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
            kind: AssignmentKind::Constant(0),
            span: span(),
        },
        Assignment {
            destination: ValueId(2),
            kind: AssignmentKind::ProductNew {
                destination: ValueId(2),
                representation: cc::ReprId(0),
                arguments: vec![ValueId(0), ValueId(1)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(3),
            kind: AssignmentKind::ArrayNew {
                destination: ValueId(3),
                representation: cc::ReprId(1),
                elements: vec![ValueId(2)],
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
        name: "FlagsListAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: vec![reference(1)],
                result: ValueShape::Integer,
            }),
        }],
        representations,
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

pub(crate) fn handle_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "handle-list.wit",
            "package wasi:io@0.2.12; interface streams { resource file; take: func(values: list<own<file>>); }",
        )
        .expect("the handle list WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![cc::Representation::Array {
            element: ValueShape::Integer,
        }],
        signatures: Vec::new(),
        product_labels: Default::default(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(1),
            ty: reference(0),
        },
        ValueDecl {
            id: ValueId(2),
            ty: ValueShape::Integer,
        },
    ];
    let assignments = vec![
        Assignment {
            destination: ValueId(0),
            kind: AssignmentKind::Constant(5),
            span: span(),
        },
        Assignment {
            destination: ValueId(1),
            kind: AssignmentKind::ArrayNew {
                destination: ValueId(1),
                representation: cc::ReprId(0),
                elements: vec![ValueId(0)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(2),
            kind: AssignmentKind::DirectCall {
                function: external_symbol,
                arguments: vec![ValueId(1)],
            },
            span: span(),
        },
    ];

    let module = cc::Module {
        name: "HandleListAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: vec![reference(0)],
                result: ValueShape::Integer,
            }),
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
            function: "take".into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}
