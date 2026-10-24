//! Result recovery is observed in guest code, independently of parameter encoding.

use super::super::fixtures::{erased, fixture, span};
use super::component;
use crate::cc::{
    AggregateConvert, Assignment, AssignmentKind as A, BoxKind, ReprId, TagCase, ValueConversion,
    ValueDecl, ValueShape, VariantCase,
};
use crate::types::ValueId;

fn scalar_result(wit: &str, fields: &[bool], wit_tag: i32, guest_tag: u32) {
    let cases = fields
        .iter()
        .enumerate()
        .map(|(tag, field)| VariantCase {
            tag: tag as u32,
            fields: if *field { vec![erased()] } else { vec![] },
        })
        .collect();
    let (mut module, bindings, resolve) = fixture(wit, "get", cases);
    let function = &mut module.functions[0];
    function.values.extend((4..8).map(|id| ValueDecl {
        id: ValueId(id),
        ty: if id == 5 {
            erased()
        } else {
            ValueShape::Integer
        },
    }));
    function.assignments.truncate(1);
    function.assignments.push(Assignment {
        destination: ValueId(4),
        kind: A::VariantTag {
            destination: ValueId(4),
            representation: ReprId(1),
            value: ValueId(0),
        },
        span: span(),
    });
    let selected = if fields[guest_tag as usize] {
        vec![
            Assignment {
                destination: ValueId(5),
                kind: A::VariantGet {
                    destination: ValueId(5),
                    representation: ReprId(1),
                    case: guest_tag,
                    field: 0,
                    value: ValueId(0),
                },
                span: span(),
            },
            Assignment {
                destination: ValueId(7),
                kind: A::AggregateConvert {
                    destination: ValueId(7),
                    value: ValueId(5),
                    conversion: AggregateConvert {
                        source: erased(),
                        destination: ValueShape::Integer,
                        plan: ValueConversion::UnboxScalar {
                            kind: BoxKind::Integer,
                            representation: ReprId(0),
                            destination: ValueShape::Integer,
                        },
                    },
                },
                span: span(),
            },
        ]
    } else {
        vec![Assignment {
            destination: ValueId(7),
            kind: A::Constant(-7),
            span: span(),
        }]
    };
    function.assignments.push(Assignment {
        destination: ValueId(3),
        kind: A::TagSwitch {
            value: ValueId(4),
            cases: vec![TagCase {
                tag: guest_tag as i32,
                assignments: selected,
                value: ValueId(7),
            }],
            default_assignments: vec![Assignment {
                destination: ValueId(6),
                kind: A::Constant(-999),
                span: span(),
            }],
            default_value: ValueId(6),
        },
        span: span(),
    });
    component::execute(
        &format!("scalar-result-{wit_tag}-{guest_tag}-{}", fields.len()),
        module,
        bindings,
        resolve,
        &format!(
            r#"(func (export "get") (param i32)
            local.get 0 i32.const {wit_tag} i32.store8
            local.get 0 i32.const 4 i32.add i32.const 42 i32.store)"#
        ),
        if fields[guest_tag as usize] { 42 } else { -7 },
    );
}

#[test]
fn option_results_execute_both_tags_and_observe_the_payload() {
    let wit = "package wasi:io@0.2.12; interface streams { get: func() -> option<s32>; }";
    scalar_result(wit, &[false, true], 0, 0);
    scalar_result(wit, &[false, true], 1, 1);
}

#[test]
fn result_results_execute_success_and_error_in_source_constructor_order() {
    let wit = "package wasi:io@0.2.12; interface streams { get: func() -> result<s32, s32>; }";
    scalar_result(wit, &[true, true], 0, 1);
    scalar_result(wit, &[true, true], 1, 0);
}

#[test]
fn variant_results_execute_each_tag_and_observe_the_payload() {
    let wit = "package wasi:io@0.2.12; interface streams { variant item { empty, number(s32) } get: func() -> item; }";
    scalar_result(wit, &[false, true], 0, 0);
    scalar_result(wit, &[false, true], 1, 1);
}
