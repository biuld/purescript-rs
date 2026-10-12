//! A valid allocation move remains fresh on every rebuild invocation/iteration.
use super::{R, binding};
use psrs_backend::cc::{Assignment, AssignmentKind as A, ValueDecl, ValueId, ValueShape};
use std::process::Command;

fn extract(
    body: &mut [Assignment],
    status: &mut Option<Assignment>,
    zero: &mut Option<Assignment>,
) -> bool {
    for assignment in body.iter_mut() {
        if let A::TagSwitch { cases, .. } = &mut assignment.kind {
            for case in cases {
                if let Some(index) = case
                    .assignments
                    .iter()
                    .position(|assignment| matches!(assignment.kind, A::ArrayNew { .. }))
                {
                    let allocation = case.assignments.remove(index);
                    let A::ArrayNew { elements, .. } = &allocation.kind else {
                        unreachable!()
                    };
                    let position = case
                        .assignments
                        .iter()
                        .position(|assignment| assignment.destination == elements[0])
                        .unwrap();
                    *zero = Some(case.assignments.remove(position));
                    *status = Some(allocation);
                    return true;
                }
                if extract(&mut case.assignments, status, zero) {
                    return true;
                }
            }
        }
    }
    false
}

#[test]
fn allocation_before_dispatch_and_filled_storage_preserve_replay_identity() {
    let Some(_) = super::super::wasmtime_available() else {
        return;
    };
    let source = include_str!("../../../tests/control_support/scoped_replay.purs").replace("AFTER", r"C.bind (raw (k 0)) (\b -> C.return { saved: a.saved, built: a.built, value: intAdd a.value b.value })");
    let mut input = crate::lower_main_to_cc(&source).unwrap();
    let symbol = binding(&input.cc.representations, R::RebuildSequence).0;
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let mut status = None;
    let mut zero = None;
    assert!(extract(&mut function.assignments, &mut status, &mut zero));
    let zero = zero.unwrap();
    assert!(matches!(zero.kind, A::Constant(0)));
    let mut status = status.unwrap();
    let A::ArrayNew {
        representation,
        elements,
        ..
    } = &status.kind
    else {
        unreachable!()
    };
    let length = ValueId(
        function
            .values
            .iter()
            .map(|declaration| declaration.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function.values.push(ValueDecl {
        id: length,
        ty: ValueShape::Integer,
    });
    status.kind = A::ArrayFill {
        destination: status.destination,
        representation: *representation,
        length,
        value: elements[0],
    };
    function.assignments.splice(
        0..0,
        [
            zero,
            Assignment {
                destination: length,
                kind: A::Constant(1),
                span: function.span,
            },
            status,
        ],
    );
    psrs_backend::cc::verify_module(&input.cc).unwrap();
    let artifact = psrs_backend::compile_cc(input).unwrap();
    let path = std::env::temp_dir().join(format!(
        "psrs-replay-filled-status-{}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, artifact.wasm).unwrap();
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
