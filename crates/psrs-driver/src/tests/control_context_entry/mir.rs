use super::{Mutation, R, SOURCE, binding};
use psrs_backend::cc::ValueId;
use psrs_backend::mir::{Instruction as I, Terminator as T};
use std::collections::HashMap;

pub(super) fn reject(mutation: Mutation) {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let symbol = binding(&stages.cc.representations, R::Executor).0;
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
    let tail = |fields: &[ValueId]| {
        matches!(
            definitions.get(&origin(fields[6])),
            Some(I::StructGet { field: 1, .. })
        )
    };
    let context_value = definitions
        .iter()
        .find_map(|(value, instruction)| match instruction {
            I::StructNew { arguments, .. } if arguments.len() == 7 && !tail(arguments) => {
                Some(*value)
            }
            _ => None,
        })
        .unwrap();
    let status = match &definitions[&context_value] {
        I::StructNew { arguments, .. } => match &definitions[&origin(arguments[6])] {
            I::StructNew { arguments, .. } => origin(arguments[1]),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    if matches!(mutation, Mutation::StatusOutsideLoop) {
        let I::ArrayNew { elements, .. } = &definitions[&status] else {
            unreachable!()
        };
        let zero = elements[0];
        assert!(matches!(definitions[&zero], I::Constant { value: 0, .. }));
        let mut moved = Vec::new();
        for block in &mut function.blocks {
            let mut index = 0;
            while index < block.instructions.len() {
                if matches!(block.instructions[index].destination(), Some(value) if value == status || value == zero)
                {
                    moved.push(block.instructions.remove(index));
                } else {
                    index += 1;
                }
            }
        }
        assert_eq!(moved.len(), 2);
        moved.sort_by_key(|instruction| usize::from(instruction.destination() == Some(status)));
        function
            .blocks
            .iter_mut()
            .find(|block| block.id == function.entry)
            .unwrap()
            .instructions
            .extend(moved);
        let errors = psrs_backend::mir::verify_module(&stages.mir).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains(mutation.message())),
            "{errors:?}"
        );
        return;
    }
    let caller = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    if matches!(mutation, Mutation::Caller) {
        let cast = ValueId(caller.0 + 1);
        let I::StructNew { type_index, .. } = definitions[&context_value] else {
            unreachable!()
        };
        let reference = psrs_backend::types::RefType {
            nullable: false,
            heap: psrs_backend::types::HeapType::Index(type_index),
        };
        let caller_ty = function
            .values
            .iter()
            .find(|value| value.id == function.parameters[5])
            .unwrap()
            .ty;
        function.values.push(psrs_backend::types::ValueDecl {
            id: caller,
            ty: caller_ty,
        });
        function.values.push(psrs_backend::types::ValueDecl {
            id: cast,
            ty: psrs_backend::types::ValueType::Ref(reference),
        });
        let entry = function
            .blocks
            .iter_mut()
            .find(|block| block.id == function.entry)
            .unwrap();
        entry.instructions.push(I::RefCast {
            destination: cast,
            value: function.parameters[5],
            reference,
            span: function.span,
        });
        entry.instructions.push(I::StructGet {
            destination: caller,
            type_index,
            field: 5,
            value: cast,
            span: function.span,
        });
    }
    let mut changed = false;
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            match mutation {
                Mutation::Caller
                | Mutation::Clauses
                | Mutation::TailQueue
                | Mutation::TailParent
                | Mutation::TailExtent => {
                    let I::StructNew { arguments, .. } = instruction else {
                        continue;
                    };
                    if arguments.len() != 7 {
                        continue;
                    }
                    match mutation {
                        Mutation::Caller => arguments[5] = caller,
                        Mutation::Clauses => arguments.swap(1, 2),
                        _ if !tail(arguments) => continue,
                        Mutation::TailQueue => arguments[3] = parameters[3],
                        Mutation::TailParent => arguments[4] = parameters[4],
                        Mutation::TailExtent => arguments[6] = parameters[4],
                        _ => unreachable!(),
                    }
                    changed = true;
                }
                Mutation::Status => {
                    let I::ArrayNew {
                        destination,
                        elements,
                        ..
                    } = instruction
                    else {
                        continue;
                    };
                    if *destination != status {
                        continue;
                    }
                    elements.clear();
                    changed = true;
                }
                _ => {}
            }
            if changed {
                break;
            }
        }
        if !changed
            && block.id != function.entry
            && let Some(T::Jump {
                target, arguments, ..
            }) = &mut block.terminator
            && *target == header
            && origin(arguments[5]) == context_value
        {
            match mutation {
                Mutation::EntryBody => arguments[2] = parameters[2],
                Mutation::EntryQueue => arguments[3] = parameters[3],
                Mutation::EntryExtent => arguments[4] = parameters[4],
                Mutation::EntryContext => arguments[5] = parameters[5],
                _ => continue,
            }
            changed = true;
        }
        if changed {
            break;
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
