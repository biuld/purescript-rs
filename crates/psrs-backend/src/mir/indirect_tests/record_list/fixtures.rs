use crate::cc::{self, Assignment, AssignmentKind, External, Signature, ValueDecl, ValueShape};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use psrs_span::TextRange;
use wit_parser::Resolve;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn reference(repr: u32) -> ValueShape {
    ValueShape::Reference(cc::Reference {
        nullable: false,
        heap: cc::RefShape::Repr(cc::ReprId(repr)),
    })
}

pub(super) fn fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "record-list.wit",
            "package wasi:io@0.2.12; interface streams { record pair { x: s32, y: f64 } take: func(values: list<pair>); }",
        )
        .expect("the record list WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            cc::Representation::Product {
                fields: vec![ValueShape::Integer, ValueShape::Number],
            },
            cc::Representation::Array {
                element: reference(0),
            },
        ],
        signatures: Vec::new(),
        product_labels: [(cc::ReprId(0), vec!["x".to_string(), "y".to_string()])]
            .into_iter()
            .collect(),
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
            ty: ValueShape::Number,
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
            kind: AssignmentKind::Constant(7),
            span: span(),
        },
        Assignment {
            destination: ValueId(1),
            kind: AssignmentKind::NumberConstant("1.5".into()),
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
        name: "RecordListAbi".into(),
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

pub(super) fn result_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "record-list.wit",
            "package wasi:io@0.2.12; interface streams { record pair { x: s32, y: f64 } get: func() -> list<pair>; }",
        )
        .expect("the record list WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            cc::Representation::Product {
                fields: vec![ValueShape::Integer, ValueShape::Number],
            },
            cc::Representation::Array {
                element: reference(0),
            },
        ],
        signatures: Vec::new(),
        product_labels: [(cc::ReprId(0), vec!["x".to_string(), "y".to_string()])]
            .into_iter()
            .collect(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let module = cc::Module {
        name: "RecordListResultAbi".into(),
        externals: vec![External {
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
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: reference(1),
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

pub(super) fn flags_fixture() -> (cc::Module, ExternalBindings, Resolve) {
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

pub(super) fn string_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "record-list.wit",
            "package wasi:io@0.2.12; interface streams { record message { text: string, code: s32 } take: func(values: list<message>); }",
        )
        .expect("the string record list WIT fixture should resolve");

    // Source record fields are sorted alphabetically: `code` then `text`. WIT
    // declares `text` before `code`, so the canonical layout and the GC struct
    // order differ and the plan must project by name.
    let representations = cc::RepresentationTable {
        representations: vec![
            cc::Representation::Product {
                fields: vec![ValueShape::Integer, ValueShape::String],
            },
            cc::Representation::Array {
                element: reference(0),
            },
        ],
        signatures: Vec::new(),
        product_labels: [(cc::ReprId(0), vec!["code".to_string(), "text".to_string()])]
            .into_iter()
            .collect(),
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
            ty: ValueShape::String,
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
            kind: AssignmentKind::Constant(7),
            span: span(),
        },
        Assignment {
            destination: ValueId(1),
            kind: AssignmentKind::StringConstant("hi".into()),
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
        name: "RecordListStringAbi".into(),
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
