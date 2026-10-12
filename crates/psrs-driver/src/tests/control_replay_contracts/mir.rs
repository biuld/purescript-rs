use super::{Mutation, R, SOURCE, binding};
use psrs_backend::cc::ValueId;
use psrs_backend::mir::{Function, Instruction as I, Terminator as T};

fn parameters(function: &Function) -> Vec<ValueId> {
    let entry = function
        .blocks
        .iter()
        .find(|block| block.id == function.entry)
        .unwrap();
    let Some(T::Jump {
        target, arguments, ..
    }) = &entry.terminator
    else {
        return function.parameters.clone();
    };
    assert_eq!(arguments, &function.parameters);
    function
        .blocks
        .iter()
        .find(|block| block.id == *target)
        .unwrap()
        .parameters
        .clone()
}

fn origin(function: &Function, mut value: ValueId) -> ValueId {
    for _ in 0..function.values.len() {
        match function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find(|instruction| instruction.destination() == Some(value))
        {
            Some(I::Copy { value: source, .. } | I::RefCast { value: source, .. }) => {
                value = *source
            }
            _ => break,
        }
    }
    value
}

pub(super) fn reject(mutation: Mutation) {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let symbol = binding(&stages.cc.representations, mutation.role()).0;
    let check = binding(&stages.cc.representations, R::Check).1;
    let close = binding(&stages.cc.representations, R::Close);
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let parameters = parameters(function);
    if matches!(mutation, Mutation::PrematureClose) {
        let instruction = function.blocks.iter_mut().flat_map(|block| &mut block.instructions).find(|instruction| matches!(instruction, I::Call { contract: Some(id), .. } if *id == check)).unwrap();
        let I::Call {
            function: target,
            contract,
            ..
        } = instruction
        else {
            unreachable!()
        };
        *target = close.0;
        *contract = Some(close.1);
    } else if matches!(mutation, Mutation::CallbackFromOtherQueue) {
        let receiver = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find_map(|instruction| match instruction {
                I::StructNew { arguments, .. } if arguments.len() == 3 => {
                    let callback = origin(function, arguments[1]);
                    match function
                        .blocks
                        .iter()
                        .flat_map(|block| &block.instructions)
                        .find(|instruction| instruction.destination() == Some(callback))
                    {
                        Some(I::StructGet { value, .. }) => Some(*value),
                        _ => None,
                    }
                }
                _ => None,
            })
            .unwrap();
        let source = function
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find_map(|instruction| match instruction {
                I::RefCast {
                    destination, value, ..
                } if *destination == receiver => Some(value),
                _ => None,
            })
            .unwrap();
        *source = parameters[1];
    } else if matches!(
        mutation,
        Mutation::WrongSavedCheck | Mutation::MissingSavedCheck
    ) {
        let instruction = function.blocks.iter_mut().flat_map(|block| &mut block.instructions).find(|instruction| matches!(instruction, I::Call { contract: Some(id), .. } if *id == check)).unwrap();
        let I::Call {
            destination,
            arguments,
            span,
            ..
        } = instruction
        else {
            unreachable!()
        };
        if matches!(mutation, Mutation::WrongSavedCheck) {
            arguments[0] = parameters[4];
        } else {
            *instruction = I::Copy {
                destination: *destination,
                value: arguments[0],
                span: *span,
            };
        }
    } else {
        let (finish, extent) = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find_map(|instruction| match instruction {
                I::StructNew {
                    destination,
                    arguments,
                    ..
                } if arguments.len() == 4 => Some((*destination, origin(function, arguments[1]))),
                _ => None,
            })
            .unwrap();
        match mutation {
            Mutation::ReusedActivation | Mutation::WrongRestoredParent => {
                let arguments = function
                    .blocks
                    .iter_mut()
                    .flat_map(|block| &mut block.instructions)
                    .find_map(|instruction| match instruction {
                        I::StructNew {
                            destination,
                            arguments,
                            ..
                        } if *destination == finish => Some(arguments),
                        _ => None,
                    })
                    .unwrap();
                if matches!(mutation, Mutation::ReusedActivation) {
                    arguments[1] = parameters[2];
                } else {
                    arguments[3] = parameters[3];
                }
            }
            Mutation::WrongActivationParent => {
                let arguments = function
                    .blocks
                    .iter_mut()
                    .flat_map(|block| &mut block.instructions)
                    .find_map(|instruction| match instruction {
                        I::StructNew {
                            destination,
                            arguments,
                            ..
                        } if *destination == extent => Some(arguments),
                        _ => None,
                    })
                    .unwrap();
                arguments[2] = parameters[2];
            }
            Mutation::ClosedStatus | Mutation::StatusOutsideLoop => {
                let status = function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .find_map(|instruction| match instruction {
                        I::StructNew {
                            destination,
                            arguments,
                            ..
                        } if *destination == extent => Some(origin(function, arguments[1])),
                        _ => None,
                    })
                    .unwrap();
                let zero = function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .find_map(|instruction| match instruction {
                        I::ArrayNew {
                            destination,
                            elements,
                            ..
                        } if *destination == status => Some(elements[0]),
                        _ => None,
                    })
                    .unwrap();
                if matches!(mutation, Mutation::ClosedStatus) {
                    let value = function
                        .blocks
                        .iter_mut()
                        .flat_map(|block| &mut block.instructions)
                        .find_map(|instruction| match instruction {
                            I::Constant {
                                destination, value, ..
                            } if *destination == zero => Some(value),
                            _ => None,
                        })
                        .unwrap();
                    assert_eq!(*value, 0);
                    *value = 1;
                } else {
                    let mut moved = Vec::new();
                    for block in &mut function.blocks {
                        let mut index = 0;
                        while index < block.instructions.len() {
                            if matches!(block.instructions[index].destination(), Some(id) if id == zero || id == status)
                            {
                                moved.push(block.instructions.remove(index));
                            } else {
                                index += 1
                            }
                        }
                    }
                    assert_eq!(moved.len(), 2);
                    moved.sort_by_key(|instruction| {
                        usize::from(instruction.destination() == Some(status))
                    });
                    function
                        .blocks
                        .iter_mut()
                        .find(|block| block.id == function.entry)
                        .unwrap()
                        .instructions
                        .extend(moved);
                }
            }
            Mutation::WrongAdvance => {
                let target = function
                    .blocks
                    .iter()
                    .find_map(|block| match &block.terminator {
                        Some(T::Jump { arguments, .. })
                            if arguments.len() == 4 && origin(function, arguments[1]) == finish =>
                        {
                            Some(block.id)
                        }
                        _ => None,
                    })
                    .unwrap();
                let Some(T::Jump { arguments, .. }) = &mut function
                    .blocks
                    .iter_mut()
                    .find(|block| block.id == target)
                    .unwrap()
                    .terminator
                else {
                    unreachable!()
                };
                arguments[2] = parameters[2];
            }
            _ => unreachable!(),
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
