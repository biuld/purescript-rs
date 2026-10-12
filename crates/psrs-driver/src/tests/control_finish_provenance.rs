//! Finish relations must reject lifetime swaps that ordinary types accept.
use psrs_backend::cc::{Assignment, AssignmentKind as A, ControlHelperRole as R};
use psrs_backend::mir::{Instruction as I, Terminator as T};

const SOURCE: &str = r"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
ret :: Int -> C.Control Op Int
ret x = C.return x
clause :: forall x. Op x -> (x -> C.Control Op Int) -> C.Control Op Int
clause (Op k) raw = C.handle ret clause (raw (k 42))
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler state (Op k) raw = { _1: state, _2: raw (k 42) }
main :: Int
main = (C.run 0 handler (C.handle ret clause (C.perform (Op \x -> x))))._2
";

fn leaf(
    body: &mut Vec<Assignment>,
    close: u32,
    edit: &mut impl FnMut(&mut Vec<Assignment>, usize),
) -> bool {
    if let Some(index) = body.iter().position(|assignment| matches!(assignment.kind, A::DirectCall { contract: Some(contract), .. } if contract.0 == close)) {
        edit(body, index);
        return true;
    }
    for assignment in body {
        if let A::TagSwitch {
            cases,
            default_assignments,
            ..
        } = &mut assignment.kind
        {
            for case in cases {
                if leaf(&mut case.assignments, close, edit) {
                    return true;
                }
            }
            if leaf(default_assignments, close, edit) {
                return true;
            }
        }
    }
    false
}

fn mutate_cc(edit: &mut impl FnMut(&mut Vec<Assignment>, usize)) -> psrs_backend::BackendInput {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let helpers = &input.cc.representations.control_helpers;
    let close = helpers
        .iter()
        .position(|helper| helper.role() == R::Close)
        .unwrap() as u32;
    let executor = helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
        .unwrap()
        .function();
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    assert!(leaf(&mut function.assignments, close, edit));
    input
}

#[test]
fn cc_rejects_closing_the_parent_instead_of_the_finish_extent() {
    let input = mutate_cc(&mut |body, index| {
        let parent = body[index + 1..]
            .iter()
            .find_map(|assignment| match &assignment.kind {
                A::DirectCall { arguments, .. } if arguments.len() == 6 => Some(arguments[4]),
                _ => None,
            })
            .unwrap();
        let A::DirectCall { arguments, .. } = &mut body[index].kind else {
            unreachable!()
        };
        arguments[0] = parent;
    });
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("must close its frame's extent")),
        "{errors:?}"
    );
}

#[test]
fn cc_rejects_restoring_the_closed_extent_instead_of_its_parent() {
    let input = mutate_cc(&mut |body, index| {
        let A::DirectCall { arguments, .. } = &body[index].kind else {
            unreachable!()
        };
        let closed = arguments[0];
        let arguments = body[index + 1..]
            .iter_mut()
            .find_map(|assignment| match &mut assignment.kind {
                A::DirectCall { arguments, .. } if arguments.len() == 6 => Some(arguments),
                _ => None,
            })
            .unwrap();
        arguments[4] = closed;
    });
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("restore the queue and parent from the same frame")),
        "{errors:?}"
    );
}

#[test]
fn optimized_mir_rejects_closing_a_same_shaped_parent() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let helpers = stages.mir.control_helpers();
    let close = helpers
        .iter()
        .position(|helper| helper.role() == R::Close)
        .unwrap() as u32;
    let executor = helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
        .unwrap()
        .function();
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let block = function.blocks.iter_mut().find(|block| block.instructions.iter().any(|instruction| matches!(instruction, I::Call { contract: Some(contract), .. } if contract.0 == close))).unwrap();
    let Some(T::Jump { arguments, .. }) = &block.terminator else {
        panic!("finish loop transition")
    };
    let parent = arguments[4];
    let call = block
        .instructions
        .iter_mut()
        .find_map(|instruction| match instruction {
            I::Call {
                arguments,
                contract: Some(contract),
                ..
            } if contract.0 == close => Some(arguments),
            _ => None,
        })
        .unwrap();
    call[0] = parent;
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("must close its frame's extent")),
        "{errors:?}"
    );
}

#[test]
fn optimized_mir_rejects_the_wrong_restored_parent() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let helpers = stages.mir.control_helpers();
    let close = helpers
        .iter()
        .position(|helper| helper.role() == R::Close)
        .unwrap() as u32;
    let executor = helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
        .unwrap()
        .function();
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let block = function.blocks.iter_mut().find(|block| block.instructions.iter().any(|instruction| matches!(instruction, I::Call { contract: Some(contract), .. } if contract.0 == close))).unwrap();
    let extent = block
        .instructions
        .iter()
        .find_map(|instruction| match instruction {
            I::Call {
                arguments,
                contract: Some(contract),
                ..
            } if contract.0 == close => Some(arguments[0]),
            _ => None,
        })
        .unwrap();
    let Some(T::Jump { arguments, .. }) = &mut block.terminator else {
        panic!("finish loop transition")
    };
    arguments[4] = extent;
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("restore the queue and parent from the same frame")),
        "{errors:?}"
    );
}

#[test]
fn cc_rejects_a_finish_disguised_as_a_valid_lifetime_check() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let helpers = &input.cc.representations.control_helpers;
    let close = helpers
        .iter()
        .position(|helper| helper.role() == R::Close)
        .unwrap() as u32;
    let check = helpers
        .iter()
        .position(|helper| helper.role() == R::Check)
        .unwrap() as u32;
    let check_function = helpers[check as usize].function();
    let executor = helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
        .unwrap()
        .function();
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    assert!(leaf(
        &mut function.assignments,
        close,
        &mut |body, index| {
            let A::DirectCall {
                contract, function, ..
            } = &mut body[index].kind
            else {
                unreachable!()
            };
            *contract = Some(psrs_backend::cc::ControlHelperId(check));
            *function = check_function;
        }
    ));
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("transition does not close its frame")),
        "{errors:?}"
    );
}

#[test]
fn optimized_mir_rejects_a_finish_without_a_close() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let helpers = stages.mir.control_helpers();
    let close = helpers
        .iter()
        .position(|helper| helper.role() == R::Close)
        .unwrap() as u32;
    let executor = helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
        .unwrap()
        .function();
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let block = function.blocks.iter_mut().find(|block| block.instructions.iter().any(|instruction| matches!(instruction, I::Call { contract: Some(contract), .. } if contract.0 == close))).unwrap();
    let index = block.instructions.iter().position(|instruction| matches!(instruction, I::Call { contract: Some(contract), .. } if contract.0 == close)).unwrap();
    block.instructions.remove(index);
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("transition does not close its frame")),
        "{errors:?}"
    );
}

#[test]
fn cc_rejects_a_parent_projected_from_another_finish_frame() {
    let input = mutate_cc(&mut |body, index| {
        let next = body[index + 1..]
            .iter()
            .find_map(|assignment| match &assignment.kind {
                A::DirectCall { arguments, .. } if arguments.len() == 6 => {
                    Some((arguments[3], arguments[4]))
                }
                _ => None,
            })
            .unwrap();
        let parent = body
            .iter_mut()
            .find(|assignment| assignment.destination == next.1)
            .unwrap();
        let A::VariantGet { value, .. } = &mut parent.kind else {
            unreachable!()
        };
        *value = next.0;
    });
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("restore the queue and parent from the same frame")),
        "{errors:?}"
    );
}

#[test]
fn optimized_mir_rejects_a_parent_from_a_different_finish_frame() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let helpers = stages.mir.control_helpers();
    let close = helpers
        .iter()
        .position(|helper| helper.role() == R::Close)
        .unwrap() as u32;
    let executor = helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
        .unwrap()
        .function();
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let block = function.blocks.iter_mut().find(|block| block.instructions.iter().any(|instruction| matches!(instruction, I::Call { contract: Some(contract), .. } if contract.0 == close))).unwrap();
    let Some(T::Jump { arguments, .. }) = &block.terminator else {
        panic!("finish loop transition")
    };
    let (queue, parent) = (arguments[3], arguments[4]);
    let cast = block
        .instructions
        .iter()
        .find_map(|instruction| match instruction {
            I::StructGet {
                destination, value, ..
            } if *destination == parent => Some(*value),
            _ => None,
        })
        .unwrap();
    let cast = block
        .instructions
        .iter_mut()
        .find(|instruction| instruction.destination() == Some(cast))
        .unwrap();
    let I::RefCast { value, .. } = cast else {
        panic!("physical finish projection")
    };
    *value = queue;
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("restore the queue and parent from the same frame")),
        "{errors:?}"
    );
}

#[test]
fn checked_reference_identity_aliases_preserve_finish_ownership() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let helpers = &input.cc.representations.control_helpers;
    let close = helpers
        .iter()
        .position(|helper| helper.role() == R::Close)
        .unwrap() as u32;
    let executor = helpers
        .iter()
        .find(|helper| helper.role() == R::Executor)
        .unwrap()
        .function();
    let queue_shape = input
        .cc
        .representations
        .signature(
            helpers
                .iter()
                .find(|helper| helper.role() == R::CloneSequence)
                .unwrap()
                .signature(),
        )
        .unwrap()
        .parameters[0];
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == executor)
        .unwrap();
    let alias = psrs_backend::cc::ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    let mut source = None;
    assert!(leaf(
        &mut function.assignments,
        close,
        &mut |body, index| {
            let parent = body[index + 1..]
                .iter()
                .find_map(|assignment| match &assignment.kind {
                    A::DirectCall { arguments, .. } if arguments.len() == 6 => Some(arguments[4]),
                    _ => None,
                })
                .unwrap();
            let definition = body
                .iter()
                .position(|assignment| assignment.destination == parent)
                .unwrap();
            let A::VariantGet { value, .. } = &mut body[definition].kind else {
                unreachable!()
            };
            source = Some(*value);
            *value = alias;
            let owner = source.unwrap();
            let span = body[definition].span;
            body.insert(
                definition,
                Assignment {
                    destination: alias,
                    span,
                    kind: A::AggregateConvert {
                        destination: alias,
                        value: owner,
                        conversion: psrs_backend::cc::AggregateConvert {
                            source: queue_shape,
                            destination: queue_shape,
                            plan: psrs_backend::cc::ValueConversion::Identity,
                        },
                    },
                },
            );
        }
    ));
    let shape = function
        .values
        .iter()
        .find(|value| value.id == source.unwrap())
        .unwrap()
        .ty;
    function.values.push(psrs_backend::cc::ValueDecl {
        id: alias,
        ty: shape,
    });
    psrs_backend::cc::verify_module(&input.cc).unwrap();
    psrs_backend::compile_cc(input).unwrap();
}
