use super::{Mutation, R, SOURCE, binding, path};
use psrs_backend::mir::{BlockId, Function, Instruction as I, Terminator as T};
use psrs_backend::types::ValueId;

fn origin(function: &Function, mut value: ValueId) -> ValueId {
    let aliases = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            I::Copy {
                destination, value, ..
            }
            | I::RefCast {
                destination, value, ..
            } => Some((*destination, *value)),
            _ => None,
        })
        .collect::<std::collections::HashMap<_, _>>();
    for _ in 0..aliases.len() {
        if let Some(source) = aliases.get(&value) {
            value = *source
        } else {
            break;
        }
    }
    value
}

fn selected(function: &Function, header: BlockId, path: &[i32]) -> BlockId {
    let mut current = header;
    for tag in path {
        loop {
            let block = function
                .blocks
                .iter()
                .find(|block| block.id == current)
                .unwrap();
            match block.terminator.as_ref().unwrap() {
                T::Switch { cases, .. } => {
                    current = cases.iter().find(|(case, _)| case == tag).unwrap().1;
                    break;
                }
                T::Jump { target, .. } => current = *target,
                _ => panic!("machine dispatch"),
            }
        }
    }
    current
}

pub(super) fn reject(mutation: Mutation) {
    use Mutation::*;
    let input = crate::lower_main_to_cc(SOURCE).unwrap();
    let (symbol, _) = binding(&input.cc.representations, R::Executor);
    let mut module = crate::compile_main_stages(SOURCE).unwrap().mir;
    let function = module
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let header = match function
        .blocks
        .iter()
        .find(|block| block.id == function.entry)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
    {
        T::Jump { target, .. } => *target,
        _ => panic!("loop header"),
    };
    let parameters = function
        .blocks
        .iter()
        .find(|block| block.id == header)
        .unwrap()
        .parameters
        .clone();
    let block_id = selected(function, header, path(mutation));
    let fresh = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function.values.push(psrs_backend::types::ValueDecl {
        id: fresh,
        ty: function.result_type,
    });
    let snapshot = function.clone();
    let block = function
        .blocks
        .iter_mut()
        .find(|block| block.id == block_id)
        .unwrap();
    let calls = block
        .instructions
        .iter()
        .filter_map(|instruction| match instruction {
            I::ClosureCall { destination, .. } => Some(*destination),
            _ => None,
        })
        .collect::<Vec<_>>();
    match mutation {
        ExtraRootCall | ExtraQueueCall | EagerBind => {
            let mut extra = if matches!(mutation, EagerBind) {
                snapshot
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .find(|instruction| matches!(instruction, I::ClosureCall { .. }))
                    .unwrap()
                    .clone()
            } else {
                block
                    .instructions
                    .iter()
                    .rfind(|instruction| matches!(instruction, I::ClosureCall { .. }))
                    .unwrap()
                    .clone()
            };
            let I::ClosureCall {
                destination,
                function: callee,
                arguments,
                ..
            } = &mut extra
            else {
                unreachable!()
            };
            *destination = fresh;
            if matches!(mutation, EagerBind) {
                *callee = parameters[1];
                *arguments = vec![parameters[0]];
            }
            block.instructions.push(extra);
        }

        HandlerState | Request | Handler | Raw | Callback | Answer => {
            let index = match mutation {
                HandlerState | Handler | Callback | Answer => 0,
                Request => 1,
                Raw => 2,
                _ => unreachable!(),
            };
            let raw = block
                .instructions
                .iter()
                .find_map(|instruction| match instruction {
                    I::ClosureNew { destination, .. } => Some(*destination),
                    _ => None,
                });
            let replacement = if matches!(mutation, HandlerState) {
                match &block
                    .instructions
                    .iter()
                    .find(|instruction| instruction.destination() == Some(calls[1]))
                    .unwrap()
                {
                    I::ClosureCall { arguments, .. } => arguments[0],
                    _ => unreachable!(),
                }
            } else if matches!(mutation, Handler) {
                raw.unwrap()
            } else if matches!(mutation, Callback) {
                parameters[1]
            } else {
                parameters[0]
            };
            let I::ClosureCall {
                function: callee,
                arguments,
                ..
            } = block
                .instructions
                .iter_mut()
                .find(|instruction| instruction.destination() == Some(calls[index]))
                .unwrap()
            else {
                unreachable!()
            };
            if matches!(mutation, Handler | Callback) {
                *callee = replacement
            } else {
                arguments[0] = replacement
            }
        }
        TrapRoot | TrapCons => {
            block.terminator = Some(T::Trap {
                span: function.span,
            });
        }
        ReturnState | ReturnAnswer => {
            let I::StructNew { arguments, .. } = block.instructions.iter_mut().find(|instruction| matches!(instruction, I::StructNew { arguments, .. } if arguments.len() == 2)).unwrap() else { unreachable!() };
            if matches!(mutation, ReturnState) {
                arguments[0] = arguments[1]
            } else {
                arguments[1] = parameters[0]
            }
        }
        _ => {
            let T::Jump {
                target, arguments, ..
            } = block.terminator.as_ref().unwrap()
            else {
                panic!("transition")
            };
            assert_eq!(*target, header);
            let next = arguments.clone();
            match mutation {
                ResultState | ResultComputation | SourceQueue | Tail | EmptyAppend
                | QueueFinishRest | QueueFinishParent => {
                    let T::Jump { arguments, .. } = block.terminator.as_mut().unwrap() else {
                        unreachable!()
                    };
                    let index = match mutation {
                        ResultState => 0,
                        ResultComputation => 2,
                        QueueFinishParent => 4,
                        _ => 3,
                    };
                    arguments[index] = parameters[index];
                }
                _ => {
                    let queue = origin(&snapshot, next[3]);
                    let I::StructNew {
                        arguments: fields, ..
                    } = block
                        .instructions
                        .iter()
                        .find(|instruction| instruction.destination() == Some(queue))
                        .unwrap()
                    else {
                        panic!("new pending queue")
                    };
                    let fields = fields.clone();
                    match mutation {
                        AppendTail | AppendRight | FinishExtent | FinishParent => {
                            let I::StructNew {
                                arguments: fields, ..
                            } = block
                                .instructions
                                .iter_mut()
                                .find(|instruction| instruction.destination() == Some(queue))
                                .unwrap()
                            else {
                                unreachable!()
                            };
                            match mutation {
                                AppendTail => fields[1] = parameters[3],
                                AppendRight => fields[2] = fields[1],
                                FinishExtent => fields[1] = fields[3],
                                FinishParent => fields[3] = fields[1],
                                _ => unreachable!(),
                            }
                        }
                        AppendFirst => {
                            let nested = origin(&snapshot, fields[2]);
                            let I::StructNew {
                                arguments: inner, ..
                            } = block
                                .instructions
                                .iter()
                                .find(|instruction| instruction.destination() == Some(nested))
                                .unwrap()
                            else {
                                unreachable!()
                            };
                            let second = inner[1];
                            let I::StructNew {
                                arguments: fields, ..
                            } = block
                                .instructions
                                .iter_mut()
                                .find(|instruction| instruction.destination() == Some(queue))
                                .unwrap()
                            else {
                                unreachable!()
                            };
                            fields[1] = second;
                        }
                        AppendSecond | FinishRest => {
                            let nested = origin(&snapshot, fields[2]);
                            let I::StructNew {
                                arguments: inner, ..
                            } = block
                                .instructions
                                .iter_mut()
                                .find(|instruction| instruction.destination() == Some(nested))
                                .unwrap()
                            else {
                                unreachable!()
                            };
                            inner[1] = if matches!(mutation, AppendSecond) {
                                fields[1]
                            } else {
                                parameters[3]
                            };
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
    let errors = psrs_backend::mir::verify_module(&module).expect_err("malformed root/queue step");
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P9 MIR root and queue execution"),
        "{mutation:?}: {errors:?}"
    );
}

mod effects;
