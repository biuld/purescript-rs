//! Synthesized aggregate fixtures. No in-scope WASI interface is simple
//! enough to drive the mapped `Either`/`Maybe`/data-type path without a library
//! wrapper, so this drives the ABI path directly.

use crate::cc::{
    self, AggregateConvert, Assignment, AssignmentKind, BoxKind, External, RefShape, Reference,
    ReprId, Representation, Signature, ValueConversion, ValueDecl, ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use psrs_span::TextRange;
use wit_parser::Resolve;

pub(super) fn span() -> TextRange {
    TextRange::new(0, 1)
}

pub(super) fn erased() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

pub(super) fn reference(repr: u32) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(ReprId(repr)),
    })
}

/// Builds a CC module whose only external returns the mapped aggregate `cases`.
/// Repr 0 is the boxed integer the erased scalar payloads are stored in; repr 1
/// is the aggregate variant supertype with erased case fields.
pub(super) fn fixture(
    wit: &str,
    function: &str,
    cases: Vec<VariantCase>,
) -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str("aggregate.wit", wit)
        .expect("the aggregate WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            Representation::Variant { cases },
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
        // Reachability anchors: a `String` value reserves the GC string type,
        // and a boxed-integer reference reserves the box the erased scalar
        // payload needs.
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
        name: "AggregateAbi".into(),
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

pub(super) fn parameter_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-parameter.wit",
            "package wasi:io@0.2.12; interface streams { take: func(value: option<s32>); }",
        )
        .expect("the parameter WIT fixture should resolve");

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
            ty: erased(),
        },
        ValueDecl {
            id: ValueId(2),
            ty: aggregate,
        },
        ValueDecl {
            id: ValueId(3),
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
            kind: AssignmentKind::AggregateConvert {
                destination: ValueId(1),
                value: ValueId(0),
                conversion: AggregateConvert {
                    source: ValueShape::Integer,
                    destination: erased(),
                    plan: ValueConversion::Sequence(vec![
                        ValueConversion::BoxScalar {
                            kind: BoxKind::Integer,
                            representation: ReprId(0),
                        },
                        ValueConversion::EraseReference,
                    ]),
                },
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(2),
            kind: AssignmentKind::VariantNew {
                destination: ValueId(2),
                representation: ReprId(1),
                case: 1,
                fields: vec![ValueId(1)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(3),
            kind: AssignmentKind::DirectCall {
                function: external_symbol,
                arguments: vec![ValueId(2)],
            },
            span: span(),
        },
    ];

    let module = cc::Module {
        name: "AggregateParameterAbi".into(),
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
            function: "take".into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}
