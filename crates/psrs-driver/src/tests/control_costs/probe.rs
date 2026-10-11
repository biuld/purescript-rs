//! Test-only probes inspect actual extent ancestry at allocation. They neither
//! alter machine layouts nor expose a source intrinsic or runtime frame API.

use psrs_backend::BackendInput;
use psrs_backend::cc::{
    Assignment, AssignmentKind as K, BinaryOp, Function, RefShape, ReprId, Representation, TagCase,
    ValueDecl, ValueId, ValueShape,
};
use psrs_hir::SymbolId;
use psrs_span::TextRange;

pub(super) fn instrument(input: &mut BackendInput, limit: i32) -> usize {
    let start = input
        .cc
        .functions
        .iter()
        .find(|f| f.name == "control_start_boundary")
        .unwrap();
    let extent = start.result_type;
    let ValueShape::Reference(reference) = extent else {
        panic!("boundary reference")
    };
    let RefShape::Repr(boundary) = reference.heap else {
        panic!("nominal boundary")
    };
    let Some(Representation::Variant { cases }) = input.cc.representations.representation(boundary)
    else {
        panic!("boundary variant")
    };
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[1].fields[1], extent);
    let module = start.symbol.module;
    let index = input
        .cc
        .functions
        .iter()
        .filter(|f| f.symbol.module == module)
        .map(|f| f.symbol.index)
        .max()
        .unwrap()
        + 1;
    let symbol = SymbolId::new(module, index);
    let mut count = 0;
    for function in &mut input.cc.functions {
        count += insert(
            &mut function.assignments,
            &mut function.values,
            boundary,
            extent,
            symbol,
            limit,
        );
    }
    input.cc.functions.push(checker(symbol, boundary, extent));
    psrs_backend::cc::verify_module(&input.cc).expect("checked ancestry probes");
    count
}

fn insert(
    body: &mut Vec<Assignment>,
    values: &mut Vec<ValueDecl>,
    boundary: ReprId,
    extent: ValueShape,
    checker: SymbolId,
    limit: i32,
) -> usize {
    let mut count = 0;
    let mut result = Vec::new();
    for mut assignment in std::mem::take(body) {
        match &mut assignment.kind {
            K::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                count += insert(then_assignments, values, boundary, extent, checker, limit);
                count += insert(else_assignments, values, boundary, extent, checker, limit);
            }
            K::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    count += insert(
                        &mut case.assignments,
                        values,
                        boundary,
                        extent,
                        checker,
                        limit,
                    );
                }
                count += insert(
                    default_assignments,
                    values,
                    boundary,
                    extent,
                    checker,
                    limit,
                );
            }
            _ => {}
        }
        let probe = matches!(&assignment.kind, K::VariantNew { representation, .. } if *representation == boundary);
        let value = assignment.destination;
        let span = assignment.span;
        result.push(assignment);
        if probe {
            count += 1;
            let next = values.iter().map(|v| v.id.0).max().unwrap() + 1;
            let budget = ValueId(next);
            let checked = ValueId(next + 1);
            values.push(ValueDecl {
                id: budget,
                ty: ValueShape::Integer,
            });
            values.push(ValueDecl {
                id: checked,
                ty: extent,
            });
            result.push(Assignment {
                destination: budget,
                kind: K::Constant(limit),
                span,
            });
            result.push(Assignment {
                destination: checked,
                kind: K::DirectCall {
                    function: checker,
                    arguments: vec![value, budget],
                },
                span,
            });
        }
    }
    *body = result;
    count
}

fn checker(symbol: SymbolId, boundary: ReprId, extent: ValueShape) -> Function {
    let span = TextRange::new(0, 1);
    let a = |id, kind| Assignment {
        destination: ValueId(id),
        kind,
        span,
    };
    let values = [
        extent,
        ValueShape::Integer,
        ValueShape::Integer,
        ValueShape::Boolean,
        ValueShape::Integer,
        extent,
        ValueShape::Integer,
        ValueShape::Integer,
        extent,
        extent,
        extent,
        extent,
        extent,
    ]
    .into_iter()
    .enumerate()
    .map(|(id, ty)| ValueDecl {
        id: ValueId(id as u32),
        ty,
    })
    .collect();
    let child = vec![
        a(
            5,
            K::VariantGet {
                destination: ValueId(5),
                representation: boundary,
                case: 1,
                field: 1,
                value: ValueId(0),
            },
        ),
        a(6, K::Constant(1)),
        a(
            7,
            K::Primitive {
                op: BinaryOp::IntSub,
                left: ValueId(1),
                right: ValueId(6),
            },
        ),
        a(
            8,
            K::DirectCall {
                function: symbol,
                arguments: vec![ValueId(5), ValueId(7)],
            },
        ),
    ];
    Function {
        symbol,
        name: "test_control_ancestry_probe".into(),
        parameters: vec![ValueId(0), ValueId(1)],
        values,
        assignments: vec![
            a(2, K::Constant(0)),
            a(
                3,
                K::Primitive {
                    op: BinaryOp::IntGe,
                    left: ValueId(1),
                    right: ValueId(2),
                },
            ),
            a(
                12,
                K::If {
                    condition: ValueId(3),
                    then_assignments: vec![
                        a(
                            4,
                            K::VariantTag {
                                destination: ValueId(4),
                                representation: boundary,
                                value: ValueId(0),
                            },
                        ),
                        a(
                            10,
                            K::TagSwitch {
                                value: ValueId(4),
                                cases: vec![
                                    TagCase {
                                        tag: 0,
                                        assignments: vec![],
                                        value: ValueId(0),
                                    },
                                    TagCase {
                                        tag: 1,
                                        assignments: child,
                                        value: ValueId(8),
                                    },
                                ],
                                default_assignments: vec![a(9, K::Unreachable)],
                                default_value: ValueId(9),
                            },
                        ),
                    ],
                    then_value: ValueId(10),
                    else_assignments: vec![a(11, K::Unreachable)],
                    else_value: ValueId(11),
                },
            ),
        ],
        result: ValueId(12),
        result_type: extent,
        span,
    }
}
