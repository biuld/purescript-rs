//! Distinct returning Runs need distinct roots; exclusive branches may share one.
use super::{R, binding};
use psrs_backend::cc::{Assignment, AssignmentKind as A, ValueId};
use psrs_backend::mir::Instruction as I;
use psrs_hir::Intrinsic;
use std::collections::HashMap;
use std::process::Command;

const HEADER: &str = r"module Main where
import Prim.Control as C
data Op x = Op (Int -> x)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler state (Op k) raw = { _1: state, _2: raw (k 42) }
";
const SEQUENTIAL: &str = "main :: Int\nmain = intAdd ((C.run 0 handler (C.return 20))._2) ((C.run 0 handler (C.return 22))._2)\n";

#[test]
fn cc_sequential_runs_cannot_share_a_root() {
    let mut input = crate::lower_main_to_cc(&format!("{HEADER}\n{SEQUENTIAL}")).unwrap();
    let close = binding(&input.cc.representations, R::Close).1;
    let table = &input.cc.representations;
    let function = input.cc.functions.iter_mut().find(|function| function.assignments.iter().filter(|assignment| matches!(&assignment.kind,
        A::ControlEvidence { contract, .. } if table.control_contracts[contract.0 as usize].operation() == Intrinsic::ControlRun)).count() == 2).unwrap();
    let calls = function
        .assignments
        .iter()
        .filter_map(|assignment| match &assignment.kind {
            A::ControlEvidence {
                contract,
                implementation,
                ..
            } if table.control_contracts[contract.0 as usize].operation()
                == Intrinsic::ControlRun =>
            {
                Some(*implementation)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let arguments = |call| match &function
        .assignments
        .iter()
        .find(|assignment| assignment.destination == call)
        .unwrap()
        .kind
    {
        A::DirectCall { arguments, .. } => arguments.clone(),
        _ => unreachable!(),
    };
    let first = arguments(calls[0]);
    let second = arguments(calls[1]);
    for assignment in &mut function.assignments {
        match &mut assignment.kind {
            A::DirectCall { arguments, .. } if assignment.destination == calls[1] => {
                arguments[4] = first[4]
            }
            A::VariantNew { fields, .. } if assignment.destination == second[5] => {
                fields[0] = first[4]
            }
            A::DirectCall {
                contract: Some(id),
                arguments,
                ..
            } if *id == close && arguments[0] == second[4] => arguments[0] = first[4],
            _ => {}
        }
    }
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("not reuse another invocation's root extent")),
        "{errors:?}"
    );
}

fn mir_reuse(status: bool) {
    let mut stages = crate::compile_main_stages(&format!("{HEADER}\n{SEQUENTIAL}")).unwrap();
    let close = binding(&stages.cc.representations, R::Close).1;
    let table = &stages.cc.representations;
    let function = stages.mir.functions.iter_mut().find(|function| function.blocks.iter().flat_map(|block| &block.instructions).filter(|instruction| matches!(instruction,
        I::ControlEvidence { contract, .. } if table.control_contracts[contract.0 as usize].operation() == Intrinsic::ControlRun)).count() == 2).unwrap();
    let definitions = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| {
            instruction
                .destination()
                .map(|value| (value, instruction.clone()))
        })
        .collect::<HashMap<_, _>>();
    let origin = |mut value: ValueId| {
        for _ in 0..definitions.len() {
            match definitions.get(&value) {
                Some(I::Copy { value: source, .. } | I::RefCast { value: source, .. }) => {
                    value = *source
                }
                _ => break,
            }
        }
        value
    };
    let calls = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            I::ControlEvidence {
                contract,
                implementation,
                ..
            } if table.control_contracts[contract.0 as usize].operation()
                == Intrinsic::ControlRun =>
            {
                Some(origin(*implementation))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let I::Call {
        arguments: first, ..
    } = &definitions[&calls[0]]
    else {
        unreachable!()
    };
    let I::Call {
        arguments: second, ..
    } = &definitions[&calls[1]]
    else {
        unreachable!()
    };
    if status {
        let I::StructNew {
            arguments: root, ..
        } = &definitions[&origin(first[4])]
        else {
            unreachable!()
        };
        let cell = root[1];
        let node = function
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| instruction.destination() == Some(origin(second[4])))
            .unwrap();
        let I::StructNew { arguments, .. } = node else {
            unreachable!()
        };
        arguments[1] = cell;
    } else {
        for instruction in function
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
        {
            match instruction {
                I::Call {
                    destination,
                    arguments,
                    ..
                } if *destination == calls[1] => arguments[4] = first[4],
                I::StructNew {
                    destination,
                    arguments,
                    ..
                } if *destination == origin(second[5]) => arguments[1] = first[4],
                I::Call {
                    contract: Some(id),
                    arguments,
                    ..
                } if *id == close && origin(arguments[0]) == origin(second[4]) => {
                    arguments[0] = first[4]
                }
                _ => {}
            }
        }
    }
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("not reuse another invocation's root extent")),
        "{errors:?}"
    );
}

fn share_branch(
    body: &mut [Assignment],
    close: psrs_backend::cc::ControlHelperId,
    extent: ValueId,
) {
    let call = body
        .iter()
        .find_map(|assignment| match &assignment.kind {
            A::DirectCall { arguments, .. } if arguments.len() == 6 => {
                Some((assignment.destination, arguments[5]))
            }
            _ => None,
        })
        .unwrap();
    for assignment in body {
        match &mut assignment.kind {
            A::DirectCall { arguments, .. } if assignment.destination == call.0 => {
                arguments[4] = extent
            }
            A::VariantNew { fields, .. } if assignment.destination == call.1 => fields[0] = extent,
            A::DirectCall {
                contract: Some(id),
                arguments,
                ..
            } if *id == close => arguments[0] = extent,
            _ => {}
        }
    }
}

#[test]
fn exclusive_run_branches_share_initialization_and_execute_independent_invocations() {
    if super::super::wasmtime_available().is_none() {
        return;
    }
    let source = format!(
        "{HEADER}\nchoose :: Boolean -> Int\nchoose flag = if flag then (C.run 0 handler (C.return 20))._2 else (C.run 0 handler (C.return 22))._2\nmain :: Int\nmain = intAdd (choose true) (choose false)\n"
    );
    let mut input = crate::lower_main_to_cc(&source).unwrap();
    let start = binding(&input.cc.representations, R::Start).1;
    let close = binding(&input.cc.representations, R::Close).1;
    let function = input.cc.functions.iter_mut().find(|function| function.assignments.iter().any(|assignment|
        matches!(&assignment.kind, A::If { then_assignments, .. } if then_assignments.iter().any(|assignment| matches!(assignment.kind, A::DirectCall { contract: Some(id), .. } if id == start))))).unwrap();
    let index = function
        .assignments
        .iter()
        .position(|assignment| matches!(assignment.kind, A::If { .. }))
        .unwrap();
    let A::If {
        then_assignments,
        else_assignments,
        ..
    } = &mut function.assignments[index].kind
    else {
        unreachable!()
    };
    let start_index = then_assignments.iter().position(|assignment| matches!(assignment.kind, A::DirectCall { contract: Some(id), .. } if id == start)).unwrap();
    let allocation = then_assignments.remove(start_index);
    let extent = allocation.destination;
    share_branch(else_assignments, close, extent);
    function.assignments.insert(index, allocation);
    psrs_backend::cc::verify_module(&input.cc).unwrap();
    let artifact = psrs_backend::compile_cc(input).unwrap();
    let path =
        std::env::temp_dir().join(format!("psrs-exclusive-root-{}.wasm", std::process::id()));
    std::fs::write(&path, artifact.wasm).unwrap();
    let output = Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn mir_sequential_runs_cannot_share_a_root() {
    mir_reuse(false);
}
#[test]
fn mir_sequential_runs_cannot_share_a_status_cell() {
    mir_reuse(true);
}
