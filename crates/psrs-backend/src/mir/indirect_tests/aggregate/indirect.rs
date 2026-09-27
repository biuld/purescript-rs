//! An indirect parameter record that carries a mapped aggregate.

use super::fixtures::{erased, reference, span};
use crate::cc::{
    self, AggregateConvert, Assignment, AssignmentKind, BoxKind, External, ReprId, Representation,
    Signature, ValueConversion, ValueDecl, ValueShape, VariantCase,
};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use wit_parser::Resolve;

/// 16 scalar parameters plus `option<s32>`, which the canonical ABI passes
/// indirectly through a parameter record.
pub(super) fn indirect_aggregate_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let scalars = 16;
    let mut wit = String::from("package wasi:io@0.2.12; interface streams { take: func(");
    for index in 0..scalars {
        wit.push_str(&format!("a{index}: s32, "));
    }
    wit.push_str("value: option<s32>); }");

    let mut resolve = Resolve::default();
    resolve
        .push_str("aggregate-indirect.wit", &wit)
        .expect("the indirect aggregate WIT fixture should resolve");

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
    let aggregate = ValueShape::Reference(crate::cc::Reference {
        nullable: false,
        heap: crate::cc::RefShape::Aggregate,
    });
    let mut values = Vec::new();
    let mut assignments = Vec::new();
    let mut arguments = Vec::new();
    let mut parameter_shapes = Vec::new();
    for index in 0..scalars {
        let id = ValueId(index as u32);
        values.push(ValueDecl {
            id,
            ty: ValueShape::Integer,
        });
        assignments.push(Assignment {
            destination: id,
            kind: AssignmentKind::Constant(index),
            span: span(),
        });
        arguments.push(id);
        parameter_shapes.push(ValueShape::Integer);
    }
    let number = ValueId(scalars as u32);
    let erased_value = ValueId(scalars as u32 + 1);
    let maybe = ValueId(scalars as u32 + 2);
    let result = ValueId(scalars as u32 + 3);
    values.push(ValueDecl {
        id: number,
        ty: ValueShape::Integer,
    });
    values.push(ValueDecl {
        id: erased_value,
        ty: erased(),
    });
    values.push(ValueDecl {
        id: maybe,
        ty: aggregate,
    });
    values.push(ValueDecl {
        id: result,
        ty: ValueShape::Integer,
    });
    assignments.push(Assignment {
        destination: number,
        kind: AssignmentKind::Constant(42),
        span: span(),
    });
    assignments.push(Assignment {
        destination: erased_value,
        kind: AssignmentKind::AggregateConvert {
            destination: erased_value,
            value: number,
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
    });
    assignments.push(Assignment {
        destination: maybe,
        kind: AssignmentKind::VariantNew {
            destination: maybe,
            representation: ReprId(1),
            case: 1,
            fields: vec![erased_value],
        },
        span: span(),
    });
    arguments.push(maybe);
    parameter_shapes.push(reference(1));
    assignments.push(Assignment {
        destination: result,
        kind: AssignmentKind::DirectCall {
            function: external_symbol,
            arguments,
        },
        span: span(),
    });

    let module = cc::Module {
        name: "IndirectAggregateAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: parameter_shapes,
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
            result,
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
