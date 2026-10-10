//! Wide scalar aggregate fixtures: 64-bit and floating payloads.

use super::fixtures::{erased, reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, ReprId, Representation, Signature, ValueDecl,
    ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

/// `result<u64, f64>` -> `Either Int Number`. The 64-bit payload is wrapped to
/// `Int` and boxed; the `f64` payload is boxed as a number.
pub(super) fn wide_scalar_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-wide.wit",
            "package wasi:io@0.2.12; interface streams { get: func() -> result<u64, f64>; }",
        )
        .expect("the wide scalar WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            Representation::Box {
                value: ValueShape::Number,
            },
            // Repr 2: the outer `Either Int Number`.
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
            ty: reference(2),
        },
        ValueDecl {
            id: ValueId(1),
            ty: reference(0),
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
        name: "WideScalarAbi".into(),
        externals: vec![External {
            projection: None,
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: reference(2),
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
        runtime: Vec::new(),
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

/// `option<f64>` -> `Maybe Number` passed as a parameter. The main function
/// builds `Just 2.5` by boxing the number into the erased field.
pub(super) fn wide_scalar_parameter_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-wide-parameter.wit",
            "package wasi:io@0.2.12; interface streams { take: func(value: option<f64>); }",
        )
        .expect("the wide scalar parameter WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Number,
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
    let aggregate = ValueShape::Reference(crate::cc::Reference {
        nullable: false,
        heap: crate::cc::RefShape::Aggregate,
    });
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueShape::Number,
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
            kind: AssignmentKind::NumberConstant("2.5".into()),
            span: span(),
        },
        Assignment {
            destination: ValueId(1),
            kind: AssignmentKind::AggregateConvert {
                destination: ValueId(1),
                value: ValueId(0),
                conversion: crate::cc::AggregateConvert {
                    source: ValueShape::Number,
                    destination: erased(),
                    plan: crate::cc::ValueConversion::Sequence(vec![
                        crate::cc::ValueConversion::BoxScalar {
                            kind: crate::cc::BoxKind::Number,
                            representation: ReprId(0),
                        },
                        crate::cc::ValueConversion::EraseReference,
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
        name: "WideScalarParameterAbi".into(),
        externals: vec![External {
            projection: None,
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
        runtime: Vec::new(),
        imports: vec![ExternalBinding {
            symbol: external_symbol,
            source_module: ModuleId(0),
            interface: "wasi:io/streams".into(),
            function: "take".into(),
            type_id: None,
            span: span(),
        }],
    };
    (module, bindings, resolve)
}
