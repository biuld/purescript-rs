//! Same-shaped lifetime helpers must retain their semantic callee identity.
use psrs_backend::cc::{Assignment, AssignmentKind, ControlHelperId, ControlHelperRole};
use psrs_backend::mir::{Instruction, Terminator};

const SOURCE: &str = r"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler state (Op k) raw = { _1: state, _2: raw (k 42) }
main :: Int
main = (C.run 0 handler (C.perform (Op \x -> x)))._2
";

fn visit(
    body: &mut [Assignment],
    check: ControlHelperId,
    replace: &mut impl FnMut(&mut AssignmentKind),
) -> bool {
    for assignment in body {
        if matches!(assignment.kind, AssignmentKind::DirectCall { contract: Some(id), .. } if id == check)
        {
            replace(&mut assignment.kind);
            return true;
        }
        if let AssignmentKind::TagSwitch {
            cases,
            default_assignments,
            ..
        } = &mut assignment.kind
        {
            for case in cases {
                if visit(&mut case.assignments, check, replace) {
                    return true;
                }
            }
            if visit(default_assignments, check, replace) {
                return true;
            }
        }
    }
    false
}

#[test]
fn cc_rejects_exchanging_check_and_close_despite_equal_signatures() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let table = &input.cc.representations;
    let check = table
        .control_helpers
        .iter()
        .position(|helper| helper.role() == ControlHelperRole::Check)
        .unwrap();
    let close = table
        .control_helpers
        .iter()
        .find(|helper| helper.role() == ControlHelperRole::Close)
        .unwrap();
    assert_eq!(
        table.signature(table.control_helpers[check].signature()),
        table.signature(close.signature())
    );
    let close = close.function();
    assert!(input.cc.functions.iter_mut().any(|function| visit(
        &mut function.assignments,
        ControlHelperId(check as u32),
        &mut |kind| {
            let AssignmentKind::DirectCall { function, .. } = kind else {
                unreachable!()
            };
            *function = close;
        }
    )));
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("predeclared semantic helper")),
        "{errors:?}"
    );
}

#[test]
fn cc_rejects_removing_a_machine_call_contract() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let check = input
        .cc
        .representations
        .control_helpers
        .iter()
        .position(|helper| helper.role() == ControlHelperRole::Check)
        .unwrap();
    assert!(input.cc.functions.iter_mut().any(|function| visit(
        &mut function.assignments,
        ControlHelperId(check as u32),
        &mut |kind| {
            let AssignmentKind::DirectCall { contract, .. } = kind else {
                unreachable!()
            };
            *contract = None;
        }
    )));
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("missing its semantic contract")),
        "{errors:?}"
    );
}

#[test]
fn optimized_mir_rejects_exchanging_check_and_close() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let check = stages
        .mir
        .control_helpers()
        .iter()
        .position(|helper| helper.role() == ControlHelperRole::Check)
        .unwrap();
    let close = stages
        .mir
        .control_helpers()
        .iter()
        .find(|helper| helper.role() == ControlHelperRole::Close)
        .unwrap()
        .function();
    let call = stages
        .mir
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match instruction {
            Instruction::Call {
                function,
                contract: Some(id),
                ..
            } if id.0 as usize == check => Some(function),
            _ => None,
        })
        .expect("optimized execution retains a lifetime check");
    *call = close;
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("predeclared semantic helper")),
        "{errors:?}"
    );
}

#[test]
fn mir_tail_calls_retain_the_predeclared_helper_identity() {
    let stages = crate::compile_main_stages(SOURCE).unwrap();
    let helpers = stages.mir.control_helpers();
    let mut count = 0;
    for block in stages
        .mir
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
    {
        if let Some(Terminator::ReturnCall {
            function,
            contract: Some(contract),
            ..
        }) = block.terminator
        {
            assert_eq!(helpers[contract.0 as usize].function(), function);
            count += 1;
        }
    }
    assert!(
        count > 0,
        "machine mutual tail calls retain their callee proof"
    );
}

#[test]
fn p9_rejects_a_wrong_helper_before_lowering_or_loopifying_calls() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let check = input
        .cc
        .representations
        .control_helpers
        .iter()
        .position(|helper| helper.role() == ControlHelperRole::Check)
        .unwrap();
    let close = input
        .cc
        .representations
        .control_helpers
        .iter()
        .find(|helper| helper.role() == ControlHelperRole::Close)
        .unwrap()
        .function();
    assert!(input.cc.functions.iter_mut().any(|function| visit(
        &mut function.assignments,
        ControlHelperId(check as u32),
        &mut |kind| {
            let AssignmentKind::DirectCall { function, .. } = kind else {
                unreachable!()
            };
            *function = close;
        }
    )));
    let errors = psrs_backend::mir::lower_module_with_bindings(
        input.cc,
        input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .err()
    .expect("P9 rejects the wrong semantic helper");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("predeclared semantic helper")),
        "{errors:?}"
    );
}

#[test]
fn mir_rejects_missing_identity_on_a_tail_helper_call() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let contract = stages
        .mir
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .find_map(|block| match &mut block.terminator {
            Some(Terminator::ReturnCall { contract, .. }) if contract.is_some() => Some(contract),
            _ => None,
        })
        .unwrap();
    *contract = None;
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("missing its semantic contract")),
        "{errors:?}"
    );
}

#[test]
fn control_helper_contracts_execute_long_requests_without_tail_call_capability() {
    let Some(()) = super::wasmtime_available() else {
        return;
    };
    let source = format!(
        r"{}
go :: Int -> C.Control Op Int
go n = if intEq n 0 then C.return 42 else C.bind (C.perform (Op \x -> x)) (\_ -> go (intSub n 1))
main :: Int
main = let result = C.run 0 handler (go 100000)
       in if intEq result._1 100000 then result._2 else 0
",
        SOURCE[..SOURCE.find("main ::").unwrap()].replace("{ _1: state,", "{ _1: intAdd state 1,")
    );
    let target = psrs_backend::TargetCapabilities {
        tail_call: false,
        ..Default::default()
    };
    let stages = crate::compile_main_with_target(&source, target).unwrap();
    assert!(
        stages
            .mir
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .all(|block| !matches!(
                block.terminator,
                Some(Terminator::ReturnCall { .. } | Terminator::ReturnCallRef { .. })
            ))
    );
    let path = std::env::temp_dir().join("psrs-control-no-tail-contracts.wasm");
    std::fs::write(&path, &stages.artifact.wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .args(["run", "-W", "tail-call=n"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
