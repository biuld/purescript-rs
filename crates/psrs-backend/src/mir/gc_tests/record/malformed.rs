use super::*;

fn read_only_fixture(short_payload: bool, kind: usize) -> CcModule {
    let mut module = roundtrip(false, short_payload, None);
    let storage = module.representations.record_storage().unwrap();
    let function = &mut module.functions[0];
    let cut = function
        .assignments
        .iter()
        .position(|assignment| {
            if short_payload {
                matches!(assignment.kind, AssignmentKind::ProductNew { representation, .. }
                if representation == storage.representation())
            } else {
                matches!(&assignment.kind, AssignmentKind::AggregateConvert { conversion, .. }
                if matches!(conversion.plan, ValueConversion::RecordPack { .. }))
            }
        })
        .unwrap();
    let record = function.assignments[cut].destination;
    function.assignments.truncate(cut + 1);
    let success = function
        .assignments
        .iter()
        .find_map(|assignment| {
            matches!(assignment.kind, AssignmentKind::Constant(42))
                .then_some(assignment.destination)
        })
        .unwrap();
    let mut add = |ty, kind: fn(ValueId) -> AssignmentKind| {
        let id = ValueId(function.values.len() as u32);
        function.values.push(ValueDecl { id, ty });
        function.assignments.push(Assignment {
            destination: id,
            kind: kind(id),
            span: span(),
        });
        id
    };
    let index = add(ValueShape::Integer, |_| AssignmentKind::Constant(0));
    let label = add(ValueShape::String, |_| {
        AssignmentKind::StringConstant("alpha".into())
    });
    let (operation, ty) = match kind {
        0 => (RecordOperation::Length, ValueShape::Integer),
        1 => (RecordOperation::LabelAt { index }, ValueShape::String),
        2 => (RecordOperation::Find { label }, ValueShape::Integer),
        3 => (
            RecordOperation::Read { index },
            crate::cc::payload::erased_shape(),
        ),
        4 => (
            RecordOperation::Update {
                replacements: Vec::new(),
            },
            reference(storage.representation()),
        ),
        _ => unreachable!(),
    };
    let destination = ValueId(function.values.len() as u32);
    function.values.push(ValueDecl {
        id: destination,
        ty,
    });
    function.assignments.push(Assignment {
        destination,
        span: span(),
        kind: AssignmentKind::Record {
            destination,
            record,
            operation,
        },
    });
    // No closed unpack follows this operation: it must enforce its own input
    // length contract even when its result is ignored (including empty Update).
    function.result = success;
    function.result_type = ValueShape::Integer;
    module
}

#[test]
fn each_record_operation_traps_on_mismatched_runtime_arrays_without_a_later_unpack() {
    for kind in 0..5 {
        for short_payload in [false, true] {
            let cc = read_only_fixture(short_payload, kind);
            crate::cc::verify_module(&cc).expect("physical CC schema remains valid");
            let (mir, _) = super::super::super::lower_module(cc).expect("record operation MIR");
            super::super::super::verify_module(&mir).expect("physical MIR remains valid");
            if let Some(output) = run_gc_output(&mir) {
                if short_payload {
                    assert!(!output.status.success(), "operation {kind}: {output:?}");
                    assert!(
                        String::from_utf8_lossy(&output.stderr).contains("unreachable"),
                        "operation {kind}: {output:?}"
                    );
                } else {
                    assert_eq!(
                        output.status.code(),
                        Some(42),
                        "operation {kind}: {output:?}"
                    );
                }
            }
        }
    }
}
