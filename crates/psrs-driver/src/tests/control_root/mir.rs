use super::{Mutation, R, SOURCE, binding};
use psrs_backend::cc::ValueId;
use psrs_backend::mir::Instruction as I;
use psrs_backend::types::ValueType;
use psrs_hir::Intrinsic;
use std::collections::HashMap;

pub(super) fn reject(mutation: Mutation) {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let close = binding(&stages.cc.representations, R::Close).1;
    let (check, check_id) = binding(&stages.cc.representations, R::Check);
    let start = binding(&stages.cc.representations, R::Start).0;
    let executor = binding(&stages.cc.representations, R::Executor).0;
    let table = &stages.cc.representations;
    let retained_start = stages
        .mir
        .functions
        .iter()
        .any(|function| function.symbol == start);
    let (finish_type, finish_ty) = stages
        .mir
        .functions
        .iter()
        .filter(|function| function.symbol == executor)
        .find_map(|function| {
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .find_map(|instruction| match instruction {
                    I::StructNew {
                        destination,
                        type_index,
                        arguments,
                        ..
                    } if arguments.len() == 4 => Some((
                        *type_index,
                        function
                            .values
                            .iter()
                            .find(|value| value.id == *destination)
                            .unwrap()
                            .ty,
                    )),
                    _ => None,
                })
        })
        .unwrap();
    let function = stages.mir.functions.iter_mut().find(|function| {
        if matches!(mutation, Mutation::StartStatus) && retained_start { return function.symbol == start; }
        function.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| matches!(instruction,
            I::ControlEvidence { contract, .. } if table.control_contracts[contract.0 as usize].operation() == Intrinsic::ControlRun))
    }).unwrap();
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
    if matches!(mutation, Mutation::StartStatus) {
        let I::ArrayNew { elements, .. } = function
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| matches!(instruction, I::ArrayNew { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        elements.clear();
    } else {
        let call = definitions
            .values()
            .find_map(|instruction| match instruction {
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
            .unwrap();
        let I::Call { arguments, .. } = &definitions[&call] else {
            unreachable!()
        };
        let extent = origin(arguments[4]);
        let context = origin(arguments[5]);
        let empty = arguments[3];
        let extra = ValueId(
            function
                .values
                .iter()
                .map(|value| value.id.0)
                .max()
                .unwrap()
                + 1,
        );
        let extent_ty = function
            .values
            .iter()
            .find(|value| value.id == extent)
            .unwrap()
            .ty;
        if matches!(
            mutation,
            Mutation::DispatchExtent | Mutation::ExecutionExtent | Mutation::WrongClose
        ) {
            function.values.push(psrs_backend::types::ValueDecl {
                id: extra,
                ty: extent_ty,
            });
            for block in &mut function.blocks {
                if let Some(index) = block
                    .instructions
                    .iter()
                    .position(|instruction| instruction.destination() == Some(extent))
                {
                    let I::StructNew {
                        type_index,
                        arguments,
                        ..
                    } = &block.instructions[index]
                    else {
                        unreachable!()
                    };
                    let node = I::StructNew {
                        destination: extra,
                        type_index: *type_index,
                        arguments: arguments.clone(),
                        span: function.span,
                    };
                    block.instructions.insert(index + 1, node);
                    break;
                }
            }
        }
        if matches!(mutation, Mutation::Queue) {
            let tag = ValueId(extra.0 + 1);
            function.values.extend([
                psrs_backend::types::ValueDecl {
                    id: extra,
                    ty: finish_ty,
                },
                psrs_backend::types::ValueDecl {
                    id: tag,
                    ty: ValueType::I32,
                },
            ]);
            for block in &mut function.blocks {
                if let Some(index) = block
                    .instructions
                    .iter()
                    .position(|instruction| instruction.destination() == Some(call))
                {
                    let I::Call { arguments, .. } = &mut block.instructions[index] else {
                        unreachable!()
                    };
                    arguments[3] = extra;
                    block.instructions.splice(
                        index..index,
                        [
                            I::Constant {
                                destination: tag,
                                value: 3,
                                span: function.span,
                            },
                            I::StructNew {
                                destination: extra,
                                type_index: finish_type,
                                arguments: vec![tag, extent, empty, extent],
                                span: function.span,
                            },
                        ],
                    );
                    break;
                }
            }
        } else {
            let mut changed = false;
            for block in &mut function.blocks {
                for instruction in &mut block.instructions {
                    match mutation {
                        Mutation::DispatchExtent => {
                            if instruction.destination() != Some(context) {
                                continue;
                            }
                            let I::StructNew { arguments, .. } = instruction else {
                                unreachable!()
                            };
                            arguments[1] = extra;
                        }
                        Mutation::ExecutionExtent => {
                            if instruction.destination() != Some(call) {
                                continue;
                            }
                            let I::Call { arguments, .. } = instruction else {
                                unreachable!()
                            };
                            arguments[4] = extra;
                        }
                        _ => {
                            let I::Call {
                                contract: Some(id),
                                arguments,
                                destination,
                                function: target,
                                span,
                                ..
                            } = instruction
                            else {
                                continue;
                            };
                            if *id != close {
                                continue;
                            }
                            match mutation {
                                Mutation::MissingClose => {
                                    *instruction = I::Copy {
                                        destination: *destination,
                                        value: arguments[0],
                                        span: *span,
                                    }
                                }
                                Mutation::WrongClose => arguments[0] = extra,
                                Mutation::CheckInsteadClose => {
                                    *id = check_id;
                                    *target = check;
                                }
                                Mutation::EarlyClose => {}
                                _ => unreachable!(),
                            }
                        }
                    }
                    changed = true;
                    break;
                }
                if changed {
                    if matches!(mutation, Mutation::EarlyClose) {
                        let close_index = block.instructions.iter().position(|instruction| matches!(instruction, I::Call { contract: Some(id), .. } if *id == close)).unwrap();
                        let call_index = block
                            .instructions
                            .iter()
                            .position(|instruction| instruction.destination() == Some(call))
                            .unwrap();
                        let instruction = block.instructions.remove(close_index);
                        block.instructions.insert(call_index, instruction);
                    }
                    break;
                }
            }
            assert!(changed);
        }
    }
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains(mutation.message())),
        "{errors:?}"
    );
}
