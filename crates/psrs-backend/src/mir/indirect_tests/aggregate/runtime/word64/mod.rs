//! Independent canonical host checks all bits, including sign and precision edges.
mod indirect;
use super::super::fixtures::{reference, span};
use super::super::scalars::wide_scalar_fixture;
use super::component;
use crate::cc::{
    self, Assignment, AssignmentKind as A, ReprId, Representation, Signature, ValueDecl, ValueShape,
};
use crate::types::ValueId;
use wit_parser::Resolve;

fn assignment(id: u32, kind: A) -> Assignment {
    Assignment {
        destination: ValueId(id),
        kind,
        span: span(),
    }
}

#[test]
fn exact_word64_round_trip_preserves_bits_in_both_directions() {
    for signed in [false, true] {
        for bits in [
            0u64,
            0xffff_ffff,
            0x1_0000_0000,
            0x20_0000_0000_0001,
            0x8000_0000_8000_0001,
            u64::MAX,
        ] {
            for field in 0..2 {
                let (mut module, mut bindings, _) = wide_scalar_fixture();
                let mut resolve = Resolve::default();
                let ty = if signed { "s64" } else { "u64" };
                resolve.push_str("word64.wit", &format!(
                    "package wasi:io@0.2.12; interface streams {{ get: func(value: {ty}) -> {ty}; }}"
                )).unwrap();
                module.representations = cc::RepresentationTable::from_parts(
                    vec![Representation::Product {
                        fields: vec![ValueShape::Integer; 2],
                    }],
                    Vec::new(),
                    Default::default(),
                );
                module.representations.product_labels =
                    [(ReprId(0), vec!["high".into(), "low".into()])]
                        .into_iter()
                        .collect();
                module.externals[0].signature = Some(Signature {
                    parameters: vec![reference(0)],
                    result: reference(0),
                });
                bindings.imports[0].function = "get".into();
                let symbol = module.externals[0].symbol;
                let high = (bits >> 32) as i32;
                let low = bits as i32;
                let function = &mut module.functions[0];
                function.values = [
                    ValueShape::Integer,
                    ValueShape::Integer,
                    reference(0),
                    reference(0),
                    ValueShape::Integer,
                ]
                .into_iter()
                .enumerate()
                .map(|(id, ty)| ValueDecl {
                    id: ValueId(id as u32),
                    ty,
                })
                .collect();
                function.assignments = vec![
                    assignment(0, A::Constant(high)),
                    assignment(1, A::Constant(low)),
                    assignment(
                        2,
                        A::ProductNew {
                            destination: ValueId(2),
                            representation: ReprId(0),
                            arguments: vec![ValueId(0), ValueId(1)],
                        },
                    ),
                    assignment(
                        3,
                        A::DirectCall {
                            contract: None,
                            function: symbol,
                            arguments: vec![ValueId(2)],
                        },
                    ),
                    assignment(
                        4,
                        A::ProductGet {
                            destination: ValueId(4),
                            representation: ReprId(0),
                            field,
                            value: ValueId(3),
                        },
                    ),
                ];
                function.result = ValueId(4);
                component::execute(
                    "word64",
                    module,
                    bindings,
                    resolve,
                    &format!(
                        r#"(func (export "get") (param i64) (result i64)
                        local.get 0 i64.const {} i64.ne if unreachable end
                        i64.const {})"#,
                        bits as i64,
                        (!bits) as i64
                    ),
                    if field == 0 {
                        ((!bits) >> 32) as i32
                    } else {
                        (!bits) as i32
                    },
                );
            }
        }
    }
}

#[test]
fn exact_word64_list_round_trip_preserves_memory_words() {
    for erased in [false, true] {
        for field in 0..2 {
            let (mut module, mut bindings, _) = wide_scalar_fixture();
            let mut resolve = Resolve::default();
            resolve.push_str("word64-list.wit", "package wasi:io@0.2.12; interface streams { get: func(value: list<u64>) -> list<u64>; }").unwrap();
            module.representations = cc::RepresentationTable::from_parts(
                vec![
                    Representation::Product {
                        fields: vec![ValueShape::Integer; 2],
                    },
                    Representation::Array {
                        element: reference(0),
                    },
                ],
                Vec::new(),
                Default::default(),
            );
            module.representations.product_labels =
                [(ReprId(0), vec!["high".into(), "low".into()])]
                    .into_iter()
                    .collect();
            module.externals[0].signature = Some(Signature {
                parameters: vec![reference(1)],
                result: reference(1),
            });
            bindings.imports[0].function = "get".into();
            let symbol = module.externals[0].symbol;
            let function = &mut module.functions[0];
            function.values = [
                ValueShape::Integer,
                ValueShape::Integer,
                reference(0),
                reference(1),
                reference(1),
                ValueShape::Integer,
                reference(0),
                ValueShape::Integer,
            ]
            .into_iter()
            .enumerate()
            .map(|(id, ty)| ValueDecl {
                id: ValueId(id as u32),
                ty,
            })
            .collect();
            function.assignments = vec![
                assignment(0, A::Constant(0x1234_5678)),
                assignment(1, A::Constant(-1)),
                assignment(
                    2,
                    A::ProductNew {
                        destination: ValueId(2),
                        representation: ReprId(0),
                        arguments: vec![ValueId(0), ValueId(1)],
                    },
                ),
                assignment(
                    3,
                    A::ArrayNew {
                        destination: ValueId(3),
                        representation: ReprId(1),
                        elements: vec![ValueId(2)],
                    },
                ),
                assignment(
                    4,
                    A::DirectCall {
                        contract: None,
                        function: symbol,
                        arguments: vec![ValueId(3)],
                    },
                ),
                assignment(5, A::Constant(0)),
                assignment(
                    6,
                    A::ArrayGet {
                        destination: ValueId(6),
                        representation: ReprId(1),
                        value: ValueId(4),
                        index: ValueId(5),
                    },
                ),
                assignment(
                    7,
                    A::ProductGet {
                        destination: ValueId(7),
                        representation: ReprId(0),
                        field,
                        value: ValueId(6),
                    },
                ),
            ];
            function.result = ValueId(7);
            if erased {
                use crate::cc::payload::{PayloadPlanner, StoragePayloadPlanner};
                module = super::super::super::record_list::storage::erased_array(module);
                let plan = StoragePayloadPlanner(&module.representations)
                    .recover_payload(reference(0), span())
                    .unwrap();
                let function = &mut module.functions[0];
                let id = function.values.len() as u32;
                function.values.push(ValueDecl {
                    id: ValueId(id),
                    ty: cc::payload::erased_shape(),
                });
                let index = function
                    .assignments
                    .iter()
                    .position(|a| matches!(a.kind, A::ArrayGet { .. }))
                    .unwrap();
                function.assignments[index].destination = ValueId(id);
                let A::ArrayGet { destination, .. } = &mut function.assignments[index].kind else {
                    unreachable!()
                };
                *destination = ValueId(id);
                function.assignments.insert(
                    index + 1,
                    assignment(
                        6,
                        A::AggregateConvert {
                            destination: ValueId(6),
                            value: ValueId(id),
                            conversion: cc::AggregateConvert {
                                source: cc::payload::erased_shape(),
                                destination: reference(0),
                                plan,
                            },
                        },
                    ),
                );
            }
            component::execute(
                "word64-list",
                module,
                bindings,
                resolve,
                r#"
            (func (export "get") (param $input i32) (param $length i32) (param $out i32) (local $data i32)
                local.get $length i32.const 1 i32.ne if unreachable end
                local.get $input i64.load i64.const 0x12345678ffffffff i64.ne if unreachable end
                i32.const 0 i32.const 0 i32.const 8 i32.const 8 call $alloc local.set $data
                local.get $data i64.const 0x8765432100000001 i64.store
                local.get $out local.get $data i32.store
                local.get $out i32.const 1 i32.store offset=4)
        "#,
                if field == 0 { 0x8765_4321u32 as i32 } else { 1 },
            );
        }
    }
}

#[test]
fn exact_word64_variant_payload_round_trip() {
    let (mut module, mut bindings, _) = wide_scalar_fixture();
    let mut resolve = Resolve::default();
    resolve.push_str("word64-option.wit", "package wasi:io@0.2.12; interface streams { get: func(value: option<u64>) -> option<u64>; }").unwrap();
    module.representations = cc::RepresentationTable::from_parts(
        vec![
            Representation::Product {
                fields: vec![ValueShape::Integer; 2],
            },
            Representation::Variant {
                cases: vec![
                    cc::VariantCase {
                        tag: 0,
                        fields: Vec::new(),
                    },
                    cc::VariantCase {
                        tag: 1,
                        fields: vec![reference(0)],
                    },
                ],
            },
        ],
        Vec::new(),
        Default::default(),
    );
    module.representations.product_labels = [(ReprId(0), vec!["high".into(), "low".into()])]
        .into_iter()
        .collect();
    module.externals[0].signature = Some(Signature {
        parameters: vec![reference(1)],
        result: reference(1),
    });
    bindings.imports[0].function = "get".into();
    let symbol = module.externals[0].symbol;
    let function = &mut module.functions[0];
    function.values = [
        ValueShape::Integer,
        ValueShape::Integer,
        reference(0),
        reference(1),
        reference(1),
        ValueShape::Integer,
        reference(0),
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Integer,
    ]
    .into_iter()
    .enumerate()
    .map(|(id, ty)| ValueDecl {
        id: ValueId(id as u32),
        ty,
    })
    .collect();
    function.assignments = vec![
        assignment(0, A::Constant(0x12345678)),
        assignment(1, A::Constant(-1)),
        assignment(
            2,
            A::ProductNew {
                destination: ValueId(2),
                representation: ReprId(0),
                arguments: vec![ValueId(0), ValueId(1)],
            },
        ),
        assignment(
            3,
            A::VariantNew {
                destination: ValueId(3),
                representation: ReprId(1),
                case: 1,
                fields: vec![ValueId(2)],
            },
        ),
        assignment(
            4,
            A::DirectCall {
                contract: None,
                function: symbol,
                arguments: vec![ValueId(3)],
            },
        ),
        assignment(
            5,
            A::VariantTag {
                destination: ValueId(5),
                representation: ReprId(1),
                value: ValueId(4),
            },
        ),
        assignment(
            9,
            A::TagSwitch {
                value: ValueId(5),
                cases: vec![cc::TagCase {
                    tag: 1,
                    assignments: vec![
                        assignment(
                            6,
                            A::VariantGet {
                                destination: ValueId(6),
                                representation: ReprId(1),
                                case: 1,
                                field: 0,
                                value: ValueId(4),
                            },
                        ),
                        assignment(
                            7,
                            A::ProductGet {
                                destination: ValueId(7),
                                representation: ReprId(0),
                                field: 0,
                                value: ValueId(6),
                            },
                        ),
                    ],
                    value: ValueId(7),
                }],
                default_assignments: vec![assignment(8, A::Constant(1))],
                default_value: ValueId(8),
            },
        ),
    ];
    function.result = ValueId(9);
    component::execute(
        "word64-option",
        module,
        bindings,
        resolve,
        r#"
        (func (export "get") (param $tag i32) (param $value i64) (param $out i32)
            local.get $tag i32.const 1 i32.ne if unreachable end
            local.get $value i64.const 0x12345678ffffffff i64.ne if unreachable end
            local.get $out i32.const 1 i32.store8
            local.get $out i64.const 0x8765432100000001 i64.store offset=8)
    "#,
        0x87654321u32 as i32,
    );
}
