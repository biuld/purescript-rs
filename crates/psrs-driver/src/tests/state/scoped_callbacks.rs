use super::*;

const SOURCE: &str = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
foreign import "psrs:intrinsic#runWorld" execute :: forall a. (State RealWorld -> Step RealWorld a) -> a
foreign import "psrs:runtime-storage#array_read" read :: forall s a. Array a -> Int -> State s -> Step s a

action :: State RealWorld -> Step RealWorld Int
action state0 =
  let next = \ignored -> read [42] 0 state0
  in next unit
main :: Int
main = execute action
"#;

#[test]
fn lexical_state_callback_has_checked_core_producers_and_execution_scope() {
    let module = lower_program_to_core(&[("Main.purs", SOURCE)]).unwrap();
    let flows = psrs_core::state::flow::check(&module).unwrap();
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].operations.len(), 1);
    flows[0].verify(&module).unwrap();
}

#[test]
fn lexical_state_callback_has_checked_cc_capture_and_invocation_producers() {
    let core = lower_program_to_core(&[("Main.purs", SOURCE)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let flows = psrs_backend::cc::state::check(&input.cc).unwrap();
    let captured = flows
        .iter()
        .find(|flow| {
            matches!(
                flow.input,
                Some(psrs_backend::cc::state::DependencySource::Capture { .. })
            )
        })
        .unwrap();
    assert_eq!(captured.operations.len(), 1);
    for flow in flows {
        flow.verify(&input.cc).unwrap();
    }
}

#[test]
fn cc_rejects_replaying_the_same_captured_dependency() {
    use psrs_backend::cc::{AssignmentKind, ValueDecl, ValueId};
    let core = lower_program_to_core(&[("Main.purs", SOURCE)]).unwrap();
    let mut module = psrs_backend::cc::lower_module(core).unwrap().cc;
    let flows = psrs_backend::cc::state::check(&module).unwrap();
    let caller = flows
        .iter()
        .find(|flow| {
            flow.operations.iter().any(|operation| {
                matches!(
                    operation.assignment.kind,
                    AssignmentKind::IndirectCall { .. }
                )
            })
        })
        .unwrap();
    let owner = caller.function.symbol;
    let mut replay = caller
        .operations
        .iter()
        .find(|operation| {
            matches!(
                operation.assignment.kind,
                AssignmentKind::IndirectCall { .. }
            )
        })
        .unwrap()
        .assignment
        .clone();
    drop(flows);
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.symbol == owner)
        .unwrap();
    let index = function
        .assignments
        .iter()
        .position(|assignment| assignment == &replay)
        .unwrap();
    let ty = function
        .values
        .iter()
        .find(|value| value.id == replay.destination)
        .unwrap()
        .ty;
    replay.destination = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function.values.push(ValueDecl {
        id: replay.destination,
        ty,
    });
    function.assignments.insert(index + 1, replay);
    let errors = psrs_backend::cc::state::check(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P8 CC state verification"
                && error.message.contains("stale dependency")),
        "{errors:?}"
    );
}

#[test]
fn captured_cc_input_evidence_cannot_be_replaced_by_an_explicit_parameter() {
    use psrs_backend::cc::state::DependencySource;
    let core = lower_program_to_core(&[("Main.purs", SOURCE)]).unwrap();
    let module = psrs_backend::cc::lower_module(core).unwrap().cc;
    let mut flows = psrs_backend::cc::state::check(&module).unwrap();
    let flow = flows
        .iter_mut()
        .find(|flow| matches!(flow.input, Some(DependencySource::Capture { .. })))
        .unwrap();
    flow.input = Some(DependencySource::Parameter(flow.function.parameters[0]));
    let errors = flow.verify(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("instruction provenance"))
    );
}

#[test]
fn captured_dependency_remains_checked_across_exclusive_cc_branches() {
    let source = SOURCE.replace(
        "read [42] 0 state0",
        "if true then read [42] 0 state0 else read [43] 0 state0",
    );
    let core = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let module = psrs_backend::cc::lower_module(core).unwrap().cc;
    let flows = psrs_backend::cc::state::check(&module).unwrap();
    let captured = flows
        .iter()
        .find(|flow| {
            matches!(
                flow.input,
                Some(psrs_backend::cc::state::DependencySource::Capture { .. })
            )
        })
        .unwrap();
    assert_eq!(captured.operations.len(), 2);
    assert_eq!(captured.graph.blocks.len(), 4);
    captured.verify(&module).unwrap();
}

#[test]
fn captured_state_callbacks_execute_with_zero_width_and_mixed_capture_slots() {
    for body in [
        "\n  let next = \\ignored -> read [42] 0 state0\n  in next unit",
        "\n  let items = [42]\n      next = \\ignored -> read items 0 state0\n  in next unit",
        "\n  let items = [42]\n      next = \\ignored -> case read [0] 0 state0 of\n        { state: state1, value: unused } -> read items 0 state1\n  in next unit",
        "\n  let items = [42]\n      index = 0\n      next = \\ignored -> case read [0] 0 state0 of\n        { state: state1, value: unused } -> read items index state1\n  in next unit",
    ] {
        let source = state_flow_source(body);
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert_eq!(output.status.code(), Some(42), "{source}\n{output:?}");
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn mir_capture_projection_rejects_type_correct_operand_and_slot_mutations() {
    use psrs_backend::{TargetCapabilities, mir};
    let source = state_flow_source(
        "\n  let items = [42]\n      other = [43]\n      next = \\ignored -> case read [0] 0 state0 of\n        { state: state1, value: unused } -> case read items 0 state1 of\n          { state: state2, value: first } -> read other 0 state2\n  in next unit",
    );
    let core = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let (module, _) =
        mir::lower_module_with_bindings(input.cc, input.externals, TargetCapabilities::default())
            .unwrap();
    mir::verify_module(&module).unwrap();
    for change_slot in [false, true] {
        let mut mutated = module.clone();
        let instruction = mutated.functions.iter_mut().flat_map(|function| &mut function.blocks)
            .flat_map(|block| &mut block.instructions).find(|instruction| if change_slot {
                matches!(instruction, mir::Instruction::ClosureGetCapture { index: 0, .. })
            } else {
                matches!(instruction, mir::Instruction::ClosureNew { captures, .. } if captures.len() == 2)
            }).unwrap();
        match instruction {
            mir::Instruction::ClosureNew { captures, .. } => captures.swap(0, 1),
            mir::Instruction::ClosureGetCapture { index, .. } => *index = 1,
            _ => unreachable!(),
        }
        let errors = mir::verify_module(&mutated).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|error| error.pass == "P9 MIR dependency verification"),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("capture projection")
                    || error.message.contains("tail conversion")),
            "{errors:?}"
        );
    }
}

#[test]
fn captured_callbacks_keep_selected_branches_and_nonreturning_operations() {
    for (flag, trap, expected) in [
        (1, false, 42),
        (2, false, 43),
        (1, true, 134),
        (2, true, 43),
    ] {
        let body = format!(
            "\n  case read [{flag}] 0 state0 of\n    {{ state: state1, value: flag }} ->\n      let items = [42]\n          other = [43]\n          next = \\ignored -> if eqInt flag 1 then read items 0 state1 else read other 0 state1\n      in next unit"
        );
        let mut source = state_flow_source(&body).replace(
            "action ::",
            "foreign import \"psrs:intrinsic#intEq\" eqInt :: Int -> Int -> Boolean\naction ::",
        );
        if trap {
            source = source.replace("action ::", "foreign import \"psrs:runtime-storage#trap\" abort :: State RealWorld -> Step RealWorld Int\naction ::")
                .replace("then read items 0 state1", "then abort state1");
        }
        let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
            return;
        };
        assert_eq!(output.status.code(), Some(expected), "{source}\n{output:?}");
        if expected == 134 {
            assert!(String::from_utf8_lossy(&output.stderr).contains("unreachable"));
        }
    }
}

#[test]
fn constructing_an_unused_trapping_capture_does_not_execute_it() {
    let source = state_flow_source("\n  let next = \\ignored -> abort state0\n  in { state: state0, value: 42 }")
        .replace("action ::", "foreign import \"psrs:runtime-storage#trap\" abort :: State RealWorld -> Step RealWorld Int\naction ::");
    let Some(output) = run_program_with_wasmtime(&[("Main.purs", &source)]) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stderr.is_empty());
}

#[test]
fn construction_does_not_execute_a_state_capturing_callback() {
    let source = SOURCE.replace("in next unit", "in { state: state0, value: 42 }");
    let module = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let flows = psrs_core::state::flow::check(&module).unwrap();
    assert_eq!(flows.len(), 1);
    assert!(flows[0].operations.is_empty());
}

#[test]
fn a_scoped_callback_cannot_replay_its_captured_predecessor() {
    let source = SOURCE.replace(
        "in next unit",
        "in case read [0] 0 state0 of\n    { state: advanced, value: ignored } -> next unit",
    );
    let errors = lower_program_to_core(&[("Main.purs", &source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage.starts_with("P7")
                && error.diagnostic.message.contains("stale dependency")),
        "{errors:?}"
    );
}

#[test]
fn nested_lexical_callback_invocation_preserves_the_captured_producer() {
    let source = SOURCE.replace(
        "in next unit",
        "    apply = \\callback -> callback unit\n  in apply next",
    );
    let module = lower_program_to_core(&[("Main.purs", &source)]).unwrap();
    let flows = psrs_core::state::flow::check(&module).unwrap();
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].operations.len(), 1);
    flows[0].verify(&module).unwrap();
}

#[test]
fn an_unchecked_global_callback_scope_cannot_discard_a_state_transition() {
    let source = SOURCE
        .replace(
            "action ::",
            "consume :: (Unit -> Step RealWorld Int) -> Int\nconsume callback = (callback unit).value\n\naction ::",
        )
        .replace(
            "in next unit",
            "    ignored = consume next\n  in { state: state0, value: ignored }",
        );
    let errors = lower_program_to_core(&[("Main.purs", &source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage.starts_with("P7")
                && error
                    .diagnostic
                    .message
                    .contains("checked invocation scope")),
        "{errors:?}"
    );
}

#[test]
fn a_choice_of_captured_callbacks_cannot_erase_dependency_provenance() {
    let source = SOURCE.replace(
        "in next unit",
        "    chosen = (if true then next else (\\ignored -> read [43] 0 state0))\n      result = chosen unit\n  in { state: state0, value: result.value }",
    );
    let errors = lower_program_to_core(&[("Main.purs", &source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage.starts_with("P7")
                && error.diagnostic.message.contains("producer provenance")),
        "{errors:?}"
    );
}

#[test]
fn an_opaque_array_cannot_hide_a_captured_state_producer() {
    let source = SOURCE.replace(
        "in next unit",
        "    callbacks = [next]\n  in { state: state0, value: 42 }",
    );
    let errors = lower_program_to_core(&[("Main.purs", &source)]).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage.starts_with("P7")
                && error
                    .diagnostic
                    .message
                    .contains("checked producer transport")),
        "{errors:?}"
    );
}
