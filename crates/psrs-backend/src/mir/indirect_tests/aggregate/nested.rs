//! Nested aggregate fixtures: a variant error payload and a record payload.

use super::fixtures::{erased, reference, span};
use crate::cc::{
    self, AggregateConvert, Assignment, AssignmentKind, External, RefShape, Reference, ReprId,
    Representation, Signature, ValueConversion, ValueDecl, ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

pub(super) fn nested_variant_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-nested.wit",
            "package wasi:io@0.2.12; interface streams { variant err { fail, code(s32) } get: func() -> result<list<u8>, err>; }",
        )
        .expect("the nested variant WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            // Repr 1: the outer `Either String Err`.
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: vec![ValueShape::String],
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![reference(2)],
                    },
                ],
            },
            // Repr 2: the nested `Err` source data type.
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
            ty: ValueShape::String,
        },
        ValueDecl {
            id: ValueId(2),
            ty: reference(0),
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
        name: "NestedAggregateAbi".into(),
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
            function: "get".into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}

pub(super) fn nested_record_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-record.wit",
            "package wasi:io@0.2.12; interface streams { record pair { x: s32, y: bool } get: func() -> option<pair>; }",
        )
        .expect("the nested record WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            // Repr 1: `Maybe Pair`.
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: Vec::new(),
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![reference(2)],
                    },
                ],
            },
            // Repr 2: the `Pair` source record.
            Representation::Product {
                fields: vec![ValueShape::Integer, ValueShape::Boolean],
            },
        ],
        signatures: Vec::new(),
        product_labels: [(ReprId(2), vec!["x".to_string(), "y".to_string()])]
            .into_iter()
            .collect(),
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
            ty: reference(2),
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
    let module = cc::Module {
        name: "NestedRecordAbi".into(),
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

/// `option<pair>` -> `Maybe Pair` passed as a parameter. The main function
/// builds `Just { x: 42, y: true }` and passes it to the import.
pub(super) fn nested_record_parameter_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-record-parameter.wit",
            "package wasi:io@0.2.12; interface streams { record pair { x: s32, y: bool } take: func(value: option<pair>); }",
        )
        .expect("the nested record parameter WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: Vec::new(),
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![reference(2)],
                    },
                ],
            },
            Representation::Product {
                fields: vec![ValueShape::Integer, ValueShape::Boolean],
            },
        ],
        signatures: Vec::new(),
        product_labels: [(ReprId(2), vec!["x".to_string(), "y".to_string()])]
            .into_iter()
            .collect(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let aggregate = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    });
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueShape::Integer,
        },
        ValueDecl {
            id: ValueId(1),
            ty: ValueShape::Boolean,
        },
        ValueDecl {
            id: ValueId(2),
            ty: reference(2),
        },
        ValueDecl {
            id: ValueId(3),
            ty: erased(),
        },
        ValueDecl {
            id: ValueId(4),
            ty: aggregate,
        },
        ValueDecl {
            id: ValueId(5),
            ty: ValueShape::Integer,
        },
    ];
    let assignments = vec![
        Assignment {
            destination: ValueId(0),
            kind: AssignmentKind::Constant(42),
            span: span(),
        },
        Assignment {
            destination: ValueId(1),
            kind: AssignmentKind::Constant(1),
            span: span(),
        },
        Assignment {
            destination: ValueId(2),
            kind: AssignmentKind::ProductNew {
                destination: ValueId(2),
                representation: ReprId(2),
                arguments: vec![ValueId(0), ValueId(1)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(3),
            kind: AssignmentKind::AggregateConvert {
                destination: ValueId(3),
                value: ValueId(2),
                conversion: AggregateConvert {
                    source: reference(2),
                    destination: erased(),
                    plan: ValueConversion::EraseReference,
                },
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(4),
            kind: AssignmentKind::VariantNew {
                destination: ValueId(4),
                representation: ReprId(1),
                case: 1,
                fields: vec![ValueId(2)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(5),
            kind: AssignmentKind::DirectCall {
                function: external_symbol,
                arguments: vec![ValueId(4)],
            },
            span: span(),
        },
    ];
    let module = cc::Module {
        name: "NestedRecordParameterAbi".into(),
        externals: vec![External {
            result_guest: None,
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
            result: ValueId(5),
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
