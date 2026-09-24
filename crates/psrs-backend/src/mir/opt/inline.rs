//! Small direct-call inlining for single-block MIR functions.

use super::effects;
use super::values::remap_instruction;
use crate::mir::{Function, Instruction, Module, Terminator};
use crate::types::ValueId;
use std::collections::HashMap;

const MAX_INLINE_INSTRUCTIONS: usize = 16;

pub(super) fn inline_small_functions(module: &mut Module) -> bool {
    let candidates = module
        .functions
        .iter()
        .filter(|function| is_candidate(function))
        .map(|function| (function.symbol, function.clone()))
        .collect::<HashMap<_, _>>();
    if candidates.is_empty() {
        return false;
    }

    let mut changed = false;
    for caller in &mut module.functions {
        let mut next_id = caller
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .map_or(Some(0), |maximum| maximum.checked_add(1));
        let caller_symbol = caller.symbol;
        let Function { blocks, values, .. } = caller;
        for block in blocks {
            let original = std::mem::take(&mut block.instructions);
            let mut rewritten = Vec::with_capacity(original.len());
            for instruction in original {
                let inline = match &instruction {
                    Instruction::Call {
                        function,
                        arguments,
                        ..
                    } if *function != caller_symbol => candidates
                        .get(function)
                        .filter(|candidate| candidate.parameters.len() == arguments.len())
                        .cloned(),
                    _ => None,
                };
                let Some(callee) = inline else {
                    rewritten.push(instruction);
                    continue;
                };

                let destination_count = callee.blocks[0]
                    .instructions
                    .iter()
                    .filter(|instruction| instruction.destination().is_some())
                    .count();
                let allocation_start = next_id;
                if destination_count > 0 {
                    let Some(start) = next_id else {
                        rewritten.push(instruction);
                        continue;
                    };
                    let Some(last_offset) = u32::try_from(destination_count - 1).ok() else {
                        rewritten.push(instruction);
                        continue;
                    };
                    let Some(last_id) = start.checked_add(last_offset) else {
                        rewritten.push(instruction);
                        continue;
                    };
                    next_id = last_id.checked_add(1);
                }

                let Instruction::Call {
                    destination,
                    arguments,
                    span,
                    ..
                } = instruction
                else {
                    unreachable!("only direct calls are inlined")
                };
                let mut mapping = callee
                    .parameters
                    .iter()
                    .copied()
                    .zip(arguments)
                    .collect::<HashMap<_, _>>();
                let mut allocation_cursor = allocation_start;
                for source in &callee.blocks[0].instructions {
                    let mut cloned = source.clone();
                    if let Some(old_destination) = source.destination() {
                        let id = allocation_cursor.expect("preflight reserved every new ID");
                        allocation_cursor = id.checked_add(1);
                        let new_destination = ValueId(id);
                        let value_type = callee
                            .values
                            .iter()
                            .find(|value| value.id == old_destination)
                            .map(|value| value.ty)
                            .expect("verified MIR definitions have value declarations");
                        values.push(crate::types::ValueDecl {
                            id: new_destination,
                            ty: value_type,
                        });
                        mapping.insert(old_destination, new_destination);
                    }
                    remap_instruction(&mut cloned, &mapping);
                    rewritten.push(cloned);
                }
                let Terminator::Return { value, .. } = callee.blocks[0]
                    .terminator
                    .as_ref()
                    .expect("candidate terminators are verified")
                else {
                    unreachable!("candidate functions return")
                };
                let result = mapping.get(value).copied().unwrap_or(*value);
                rewritten.push(Instruction::Copy {
                    destination,
                    value: result,
                    span,
                });
                changed = true;
            }
            block.instructions = rewritten;
        }
    }
    changed
}

fn is_candidate(function: &Function) -> bool {
    if function.blocks.len() != 1 || !function.blocks[0].parameters.is_empty() {
        return false;
    }
    let block = &function.blocks[0];
    if block.instructions.len() > MAX_INLINE_INSTRUCTIONS
        || !matches!(block.terminator, Some(Terminator::Return { .. }))
    {
        return false;
    }
    // Keep calls as calls. The cloned body may contain traps or memory effects;
    // those stay at the original call position and are not reordered.
    block
        .instructions
        .iter()
        .all(|instruction| !effects::classify(instruction).may_call)
}
