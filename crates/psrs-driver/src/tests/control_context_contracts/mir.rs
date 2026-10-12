use super::{Mutation, R, SOURCE, binding};
use psrs_backend::cc::ValueId;
use psrs_backend::mir::{Instruction as I, Terminator as T};
use std::collections::HashMap;

pub(super) fn reject(mutation: Mutation) {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let symbol = binding(&stages.cc.representations, mutation.role()).0;
    let clone = binding(&stages.cc.representations, R::CloneSequence).1;
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let entry = function
        .blocks
        .iter()
        .find(|block| block.id == function.entry)
        .unwrap();
    let Some(T::Jump {
        target: header,
        arguments,
        ..
    }) = &entry.terminator
    else {
        unreachable!()
    };
    assert_eq!(arguments, &function.parameters);
    let header = *header;
    let parameters = function
        .blocks
        .iter()
        .find(|block| block.id == header)
        .unwrap()
        .parameters
        .clone();
    let definitions = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| {
            instruction
                .destination()
                .map(|destination| (destination, instruction.clone()))
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
    let context_extent = |value| matches!(definitions.get(&origin(value)), Some(I::StructGet { field: 6, value, .. }) if origin(*value) == parameters[5]);
    let target_finish = if parameters.len() == 6 {
        definitions
            .iter()
            .find_map(|(value, instruction)| match instruction {
                I::StructNew { arguments, .. }
                    if arguments.len() == 4 && context_extent(arguments[1]) =>
                {
                    Some(*value)
                }
                _ => None,
            })
    } else {
        None
    };
    let copied_append = if matches!(mutation, Mutation::CopyOrder) {
        definitions
            .values()
            .find_map(|instruction| match instruction {
                I::StructNew { arguments, .. }
                    if arguments.len() == 4 && !context_extent(arguments[1]) =>
                {
                    Some(origin(arguments[2]))
                }
                _ => None,
            })
    } else {
        None
    };
    let mut changed = false;
    match mutation {
        Mutation::FinishExtent
        | Mutation::FinishQueue
        | Mutation::FinishParent
        | Mutation::CopyParent
        | Mutation::CopyOrder => {
            for instruction in function
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.instructions)
            {
                let I::StructNew {
                    destination,
                    arguments,
                    ..
                } = instruction
                else {
                    continue;
                };
                let target = match mutation {
                    Mutation::CopyParent => arguments.len() == 4 && !context_extent(arguments[1]),
                    Mutation::CopyOrder => Some(*destination) == copied_append,
                    _ => Some(*destination) == target_finish,
                };
                if !target {
                    continue;
                }
                match mutation {
                    Mutation::FinishExtent => arguments[1] = parameters[4],
                    Mutation::FinishQueue => arguments[2] = parameters[3],
                    Mutation::FinishParent | Mutation::CopyParent => arguments[3] = parameters[4],
                    Mutation::CopyOrder => arguments.swap(1, 2),
                    _ => unreachable!(),
                }
                changed = true;
                break;
            }
        }
        Mutation::SeedParent | Mutation::SeedActive | Mutation::SeedAccumulator => {
            for instruction in function
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.instructions)
            {
                if let I::Call {
                    contract: Some(id),
                    arguments,
                    ..
                } = instruction
                {
                    if *id != clone {
                        continue;
                    }
                    match mutation {
                        Mutation::SeedParent => arguments[4] = parameters[4],
                        Mutation::SeedActive => arguments[3] = arguments[4],
                        Mutation::SeedAccumulator => arguments[1] = arguments[0],
                        _ => unreachable!(),
                    }
                    changed = true;
                    break;
                }
            }
        }
        Mutation::CallerContext | Mutation::CloneForward | Mutation::CallbackForward => {
            for block in &mut function.blocks {
                if block.id == function.entry {
                    continue;
                }
                let Some(T::Jump {
                    target, arguments, ..
                }) = &mut block.terminator
                else {
                    continue;
                };
                if *target != header {
                    continue;
                }
                let target = match mutation {
                    Mutation::CallerContext => Some(origin(arguments[3])) == target_finish,
                    Mutation::CloneForward => true,
                    Mutation::CallbackForward => {
                        matches!(definitions.get(&origin(arguments[1])), Some(I::StructNew { arguments, .. }) if arguments.len() == 3 && matches!(definitions.get(&origin(arguments[1])), Some(I::StructGet { field: 1, .. })))
                    }
                    _ => unreachable!(),
                };
                if !target {
                    continue;
                }
                match mutation {
                    Mutation::CallerContext => arguments[5] = parameters[5],
                    Mutation::CloneForward => arguments[4] = parameters[3],
                    Mutation::CallbackForward => arguments[3] = parameters[2],
                    _ => unreachable!(),
                }
                changed = true;
                break;
            }
        }
    }
    assert!(changed);
    let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains(mutation.message())),
        "{errors:?}"
    );
}
