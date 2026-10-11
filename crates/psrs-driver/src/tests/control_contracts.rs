//! Structural contracts are checked where control lowering creates them.

use psrs_backend::cc::{AssignmentKind, Function, RefShape, Representation, ValueId, ValueShape};

fn shape(function: &Function, value: ValueId) -> ValueShape {
    function
        .values
        .iter()
        .find(|declaration| declaration.id == value)
        .unwrap()
        .ty
}

#[test]
fn control_machine_preserves_nominal_capture_and_replay_contracts() {
    let input = crate::lower_main_to_cc(
        r"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler state (Op k) raw = { _1: state, _2: raw (k 42) }
main :: Int
main = (C.run 0 handler (C.perform (Op \x -> x)))._2
",
    )
    .expect("checked source should lower to CC");
    let find = |name| {
        input
            .cc
            .functions
            .iter()
            .find(|function| function.name == name)
            .unwrap()
    };
    let executor = find("control_executor");
    let queue = shape(executor, executor.parameters[3]);
    let boundary = shape(executor, executor.parameters[4]);
    assert!(
        matches!(queue, ValueShape::Reference(reference) if matches!(reference.heap, RefShape::Repr(_)))
    );
    assert!(
        matches!(boundary, ValueShape::Reference(reference) if matches!(reference.heap, RefShape::Repr(_)))
    );
    assert_ne!(queue, boundary);

    let resume = find("control_resume");
    let captures = resume
        .assignments
        .iter()
        .filter_map(|assignment| {
            if let AssignmentKind::ClosureGetCapture { index, .. } = &assignment.kind {
                Some((*index, shape(resume, assignment.destination)))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(captures, vec![(0, queue), (1, boundary)]);

    let clone = find("control_clone_sequence");
    let rebuild = find("control_rebuild_sequence");
    let parameters = |function: &Function| {
        function
            .parameters
            .iter()
            .map(|value| shape(function, *value))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        parameters(clone),
        vec![queue, queue, queue, boundary, boundary]
    );
    assert_eq!(parameters(rebuild), vec![queue, queue, boundary, boundary]);
    assert_eq!(clone.result_type, rebuild.result_type);
    let ValueShape::Reference(reference) = clone.result_type else {
        panic!("replay result reference")
    };
    let RefShape::Repr(replay) = reference.heap else {
        panic!("nominal replay result")
    };
    assert_eq!(
        input.cc.representations.representation(replay),
        Some(&Representation::Product {
            fields: vec![queue, boundary]
        })
    );
    for name in ["control_check_boundary", "control_close_boundary"] {
        let function = find(name);
        assert_eq!(parameters(function), vec![boundary]);
        assert_eq!(function.result_type, boundary);
    }
}
