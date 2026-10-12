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

const DOMAINS: &str = r"module Main where
import Prim.Control as C
data F x = F (Int -> x)
data G x = G (Int -> x)
clause :: forall x. F x -> (x -> C.Control F Int) -> C.Control G Boolean
clause request raw = C.return true
outer :: forall x. Int -> G x -> (x -> C.Control G Boolean) -> { _1 :: Int, _2 :: C.Control G Boolean }
outer state (G k) raw = { _1: state, _2: raw (k 0) }
main :: Int
main = let target = (C.return true :: C.Control G Boolean)
           result = C.handle (\n -> target) clause (C.return 42)
       in if (C.run 0 outer result)._2 then 42 else 0
";

#[test]
fn checked_domains_and_rank_two_clause_survive_physical_lowering() {
    use psrs_backend::cc::{TypeConstructorIdentity as C, TypeIdentity as T};
    let stages = crate::compile_main_stages(DOMAINS).unwrap();
    let cc = &stages.cc;
    let handle = cc
        .representations
        .control_contracts
        .iter()
        .find(|proof| proof.operation() == psrs_hir::Intrinsic::ControlHandle)
        .unwrap();
    assert_eq!(handle.source().unwrap().answer, T::Constructor(C::Int));
    assert_eq!(handle.target().unwrap().answer, T::Constructor(C::Boolean));
    assert_ne!(
        handle.source().unwrap().request,
        handle.target().unwrap().request
    );
    assert!(matches!(handle.arguments()[1], T::ForAll { .. }));
    let mir = &stages.mir;
    assert!(mir.control_contracts().iter().any(|proof| proof == handle));
    assert!(
        mir.functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(
                instruction,
                psrs_backend::mir::Instruction::ControlEvidence { .. }
            ))
    );
    psrs_backend::mir::verify_module(mir).unwrap();
}

#[test]
fn cc_rejects_a_dangling_control_contract() {
    let mut input = crate::lower_main_to_cc(DOMAINS).unwrap();
    let evidence = input
        .cc
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.assignments)
        .find_map(|assignment| match &mut assignment.kind {
            AssignmentKind::ControlEvidence { contract, .. } => Some(contract),
            _ => None,
        })
        .unwrap();
    *evidence = psrs_backend::cc::ControlContractId(u32::MAX);
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("no checked source contract"))
    );
}

#[test]
fn cc_rejects_a_same_shape_body_from_the_target_domain() {
    let mut input = crate::lower_main_to_cc(DOMAINS).unwrap();
    let contracts = &input.cc.representations.control_contracts;
    let (function, index) = input.cc.functions.iter_mut().find_map(|function| function.assignments.iter().position(|assignment| matches!(&assignment.kind,
        AssignmentKind::ControlEvidence { contract, .. } if contracts[contract.0 as usize].operation() == psrs_hir::Intrinsic::ControlHandle)).map(|index| (function, index))).unwrap();
    let target = function
        .assignments
        .iter()
        .find_map(|assignment| match &assignment.kind {
            AssignmentKind::ControlEvidence { contract, .. }
                if contracts[contract.0 as usize]
                    .target()
                    .is_some_and(|domain| {
                        domain.answer
                            == psrs_backend::cc::TypeIdentity::Constructor(
                                psrs_backend::cc::TypeConstructorIdentity::Boolean,
                            )
                    }) =>
            {
                Some(assignment.destination)
            }
            _ => None,
        })
        .unwrap();
    let implementation = match &mut function.assignments[index].kind {
        AssignmentKind::ControlEvidence {
            arguments,
            implementation,
            ..
        } => {
            arguments[2] = target;
            *implementation
        }
        _ => unreachable!(),
    };
    let frame = function
        .assignments
        .iter_mut()
        .find(|assignment| assignment.destination == implementation)
        .unwrap();
    let AssignmentKind::VariantNew { fields, .. } = &mut frame.kind else {
        unreachable!()
    };
    fields[2] = target;
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("source type disagrees with its checked domain")),
        "{errors:?}"
    );
}

#[test]
fn cc_rejects_a_frame_that_does_not_save_the_proven_source_body() {
    let mut input = crate::lower_main_to_cc(DOMAINS).unwrap();
    let contracts = &input.cc.representations.control_contracts;
    let function = input.cc.functions.iter_mut().find(|function| function.assignments.iter().any(|assignment| matches!(&assignment.kind,
        AssignmentKind::ControlEvidence { contract, .. } if contracts[contract.0 as usize].operation() == psrs_hir::Intrinsic::ControlHandle))).unwrap();
    let implementation = function
        .assignments
        .iter()
        .find_map(|assignment| match &assignment.kind {
            AssignmentKind::ControlEvidence {
                contract,
                implementation,
                ..
            } if contracts[contract.0 as usize].operation()
                == psrs_hir::Intrinsic::ControlHandle =>
            {
                Some(*implementation)
            }
            _ => None,
        })
        .unwrap();
    let other = function
        .assignments
        .iter()
        .find_map(|assignment| match &assignment.kind {
            AssignmentKind::ControlEvidence { contract, .. }
                if contracts[contract.0 as usize].operation()
                    == psrs_hir::Intrinsic::ControlReturn
                    && contracts[contract.0 as usize]
                        .target()
                        .is_some_and(|domain| {
                            domain.answer
                                == psrs_backend::cc::TypeIdentity::Constructor(
                                    psrs_backend::cc::TypeConstructorIdentity::Boolean,
                                )
                        }) =>
            {
                Some(assignment.destination)
            }
            _ => None,
        })
        .unwrap();
    let frame = function
        .assignments
        .iter_mut()
        .find(|assignment| assignment.destination == implementation)
        .unwrap();
    let AssignmentKind::VariantNew { fields, .. } = &mut frame.kind else {
        unreachable!()
    };
    fields[2] = other;
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("does not preserve its checked source operands")),
        "{errors:?}"
    );
}

#[test]
fn mir_rejects_an_operand_from_the_other_checked_domain() {
    let mut stages = crate::compile_main_stages(DOMAINS).unwrap();
    let contracts = stages.mir.control_contracts().to_vec();
    let function = stages.mir.functions.iter_mut().find(|function| function.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(instruction,
        psrs_backend::mir::Instruction::ControlEvidence { contract, .. } if contracts[contract.0 as usize].operation() == psrs_hir::Intrinsic::ControlHandle))).unwrap();
    let target = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction {
            psrs_backend::mir::Instruction::ControlEvidence {
                destination,
                contract,
                ..
            } if contracts[contract.0 as usize].operation()
                == psrs_hir::Intrinsic::ControlReturn
                && contracts[contract.0 as usize]
                    .target()
                    .is_some_and(|domain| {
                        domain.answer
                            == psrs_backend::cc::TypeIdentity::Constructor(
                                psrs_backend::cc::TypeConstructorIdentity::Boolean,
                            )
                    }) =>
            {
                Some(*destination)
            }
            _ => None,
        })
        .unwrap();
    for instruction in function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
    {
        if let psrs_backend::mir::Instruction::ControlEvidence {
            arguments,
            contract,
            ..
        } = instruction
            && contracts[contract.0 as usize].operation() == psrs_hir::Intrinsic::ControlHandle
        {
            arguments[2] = target;
        }
    }
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("source type disagrees with its checked domain")),
        "{errors:?}"
    );
}

#[test]
fn mir_rejects_a_dangling_control_contract_after_optimization() {
    let mut stages = crate::compile_main_stages(DOMAINS).unwrap();
    let contract = stages
        .mir
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| match instruction {
            psrs_backend::mir::Instruction::ControlEvidence { contract, .. } => Some(contract),
            _ => None,
        })
        .unwrap();
    *contract = psrs_backend::cc::ControlContractId(u32::MAX);
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("no checked source contract"))
    );
}
