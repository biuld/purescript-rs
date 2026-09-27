//! A record payload whose field is itself an aggregate.

use super::fixtures::{erased, reference, span};
use crate::cc::{
    self, Assignment, AssignmentKind, External, ReprId, Representation, Signature, ValueDecl,
    ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

/// `result<outer, s32>` -> `Either Outer Int` where
/// `Outer = { inner :: Maybe Int }`.
pub(super) fn record_with_aggregate_field_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-record-field.wit",
            "package wasi:io@0.2.12; interface streams { record outer { inner: option<s32> } get: func() -> result<outer, s32>; }",
        )
        .expect("the record field WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            // Repr 1: `Maybe Int`.
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
            // Repr 2: `Outer`.
            Representation::Product {
                fields: vec![reference(1)],
            },
            // Repr 3: `Either Outer Int`.
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: vec![reference(2)],
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![ValueShape::Integer],
                    },
                ],
            },
        ],
        signatures: Vec::new(),
        product_labels: [(ReprId(2), vec!["inner".to_string()])]
            .into_iter()
            .collect(),
    };

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: reference(3),
        },
        ValueDecl {
            id: ValueId(1),
            ty: reference(1),
        },
        ValueDecl {
            id: ValueId(2),
            ty: reference(2),
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
        name: "RecordFieldAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: Vec::new(),
                result: reference(3),
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

/// `option<outer>` -> `Maybe Outer` passed as a parameter, where
/// `Outer = { inner :: Maybe Int }`. The main function builds
/// `Just { inner: Just 42 }`.
pub(super) fn record_with_aggregate_field_parameter_fixture()
-> (cc::Module, ExternalBindings, Resolve) {
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "aggregate-record-field-parameter.wit",
            "package wasi:io@0.2.12; interface streams { record outer { inner: option<s32> } take: func(value: option<outer>); }",
        )
        .expect("the record field parameter WIT fixture should resolve");

    let representations = cc::RepresentationTable {
        representations: vec![
            Representation::Box {
                value: ValueShape::Integer,
            },
            // Repr 1: `Maybe Int`, the `inner` field type.
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: Vec::new(),
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![ValueShape::Integer],
                    },
                ],
            },
            // Repr 2: `Outer`.
            Representation::Product {
                fields: vec![reference(1)],
            },
            // Repr 3: the monomorphic `Maybe Outer` the import receives.
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
        ],
        signatures: Vec::new(),
        product_labels: [(ReprId(2), vec!["inner".to_string()])]
            .into_iter()
            .collect(),
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
            ty: reference(1),
        },
        ValueDecl {
            id: ValueId(4),
            ty: reference(2),
        },
        ValueDecl {
            id: ValueId(5),
            ty: erased(),
        },
        ValueDecl {
            id: ValueId(6),
            ty: aggregate,
        },
        ValueDecl {
            id: ValueId(7),
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
                conversion: crate::cc::AggregateConvert {
                    source: ValueShape::Integer,
                    destination: erased(),
                    plan: crate::cc::ValueConversion::Sequence(vec![
                        crate::cc::ValueConversion::BoxScalar {
                            kind: crate::cc::BoxKind::Integer,
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
                fields: vec![ValueId(0)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(3),
            kind: AssignmentKind::AggregateConvert {
                destination: ValueId(3),
                value: ValueId(2),
                conversion: crate::cc::AggregateConvert {
                    source: aggregate,
                    destination: reference(1),
                    plan: crate::cc::ValueConversion::RecoverReference {
                        destination: reference(1),
                        evidence: crate::cc::RecoveryEvidence::TypeInstantiation,
                    },
                },
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(4),
            kind: AssignmentKind::ProductNew {
                destination: ValueId(4),
                representation: ReprId(2),
                arguments: vec![ValueId(3)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(5),
            kind: AssignmentKind::AggregateConvert {
                destination: ValueId(5),
                value: ValueId(4),
                conversion: crate::cc::AggregateConvert {
                    source: reference(2),
                    destination: erased(),
                    plan: crate::cc::ValueConversion::EraseReference,
                },
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(6),
            kind: AssignmentKind::VariantNew {
                destination: ValueId(6),
                representation: ReprId(3),
                case: 1,
                fields: vec![ValueId(4)],
            },
            span: span(),
        },
        Assignment {
            destination: ValueId(7),
            kind: AssignmentKind::DirectCall {
                function: external_symbol,
                arguments: vec![ValueId(6)],
            },
            span: span(),
        },
    ];
    let module = cc::Module {
        name: "RecordFieldParameterAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: vec![reference(3)],
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
            result: ValueId(7),
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
