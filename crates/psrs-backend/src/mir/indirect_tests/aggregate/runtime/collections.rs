//! Observe aggregate fields and list elements after canonical result recovery.

use super::super::{
    collections::non_byte_list_fixture,
    fixtures::{reference, span},
    nested::nested_record_fixture,
};
use super::component;
use crate::cc::{self, Assignment, AssignmentKind as A, ReprId, TagCase, ValueDecl, ValueShape};
use crate::types::ValueId;

fn assignment(id: u32, kind: A) -> Assignment {
    Assignment {
        destination: ValueId(id),
        kind,
        span: span(),
    }
}

fn observe(
    module: &mut cc::Module,
    representation: u32,
    payload: ValueShape,
    body: Vec<Assignment>,
    result: u32,
) {
    let function = &mut module.functions[0];
    // The original fixtures reserve values 0..3; use fresh, consistently typed IDs.
    function.values.extend((4..12).map(|id| ValueDecl {
        id: ValueId(id),
        ty: if id == 5 {
            payload
        } else {
            ValueShape::Integer
        },
    }));
    function.assignments.truncate(1);
    function.assignments.push(assignment(
        4,
        A::VariantTag {
            destination: ValueId(4),
            representation: ReprId(representation),
            value: ValueId(0),
        },
    ));
    let mut selected = vec![assignment(
        5,
        A::VariantGet {
            destination: ValueId(5),
            representation: ReprId(representation),
            case: 1,
            field: 0,
            value: ValueId(0),
        },
    )];
    selected.extend(body);
    function.assignments.push(assignment(
        11,
        A::TagSwitch {
            value: ValueId(4),
            cases: vec![TagCase {
                tag: 1,
                assignments: selected,
                value: ValueId(result),
            }],
            default_assignments: vec![assignment(10, A::Constant(-999))],
            default_value: ValueId(10),
        },
    ));
    function.result = ValueId(11);
}

#[test]
fn list_result_executes_and_observes_length_and_element() {
    let (mut module, bindings, resolve) = non_byte_list_fixture();
    observe(
        &mut module,
        0,
        reference(1),
        vec![
            assignment(
                6,
                A::ArrayLen {
                    destination: ValueId(6),
                    value: ValueId(5),
                },
            ),
            assignment(7, A::Constant(1)),
            assignment(
                8,
                A::ArrayGet {
                    destination: ValueId(8),
                    representation: ReprId(1),
                    value: ValueId(5),
                    index: ValueId(7),
                },
            ),
            assignment(
                9,
                A::Primitive {
                    op: cc::BinaryOp::IntAdd,
                    left: ValueId(6),
                    right: ValueId(8),
                },
            ),
        ],
        9,
    );
    component::execute(
        "list-result",
        module,
        bindings,
        resolve,
        r#"(func (export "get") (param $out i32) (local $data i32)
            i32.const 0 i32.const 0 i32.const 4 i32.const 12 call $alloc local.set $data
            local.get $data i32.const 7 i32.store
            local.get $data i32.const 42 i32.store offset=4
            local.get $data i32.const 9 i32.store offset=8
            local.get $out i32.const 1 i32.store8
            local.get $out local.get $data i32.store offset=4
            local.get $out i32.const 3 i32.store offset=8)"#,
        45,
    );
}

#[test]
fn record_result_executes_and_observes_a_field() {
    let (mut module, bindings, resolve) = nested_record_fixture();
    // This fixture reserves only 0..2; keep the observer's fresh range explicit.
    module.functions[0].values.push(ValueDecl {
        id: ValueId(3),
        ty: ValueShape::Integer,
    });
    observe(
        &mut module,
        1,
        reference(2),
        vec![assignment(
            6,
            A::ProductGet {
                destination: ValueId(6),
                representation: ReprId(2),
                field: 0,
                value: ValueId(5),
            },
        )],
        6,
    );
    component::execute(
        "record-result",
        module,
        bindings,
        resolve,
        r#"(func (export "get") (param i32)
            local.get 0 i32.const 1 i32.store8
            local.get 0 i32.const 73 i32.store offset=4
            local.get 0 i32.const 1 i32.store8 offset=8)"#,
        73,
    );
}
