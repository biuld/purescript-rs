use crate::abi::REALLOC_SYMBOL;
use crate::mir::{Function, Instruction, NumericOp, Terminator};
use crate::types::ValueId;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AddressFact {
    Pending,
    Known(u32),
    /// Writable allocation returned by the ABI allocator, sized by its
    /// statically known `new_len` argument.
    Allocated(u32),
    Unknown,
}

pub(super) fn solve_address_facts(
    function: &Function,
    string_offsets: &HashMap<String, u32>,
) -> HashMap<ValueId, AddressFact> {
    let mut facts = function
        .values
        .iter()
        .map(|value| (value.id, AddressFact::Unknown))
        .collect::<HashMap<_, _>>();
    for block in &function.blocks {
        for parameter in &block.parameters {
            facts.insert(*parameter, AddressFact::Pending);
        }
        for instruction in &block.instructions {
            if let Some(destination) = instruction.destination() {
                let fact = match instruction {
                    Instruction::Constant { value, .. } => AddressFact::Known(*value as u32),
                    Instruction::StringConstant { bytes, .. } => string_offsets
                        .get(bytes)
                        .copied()
                        .map(AddressFact::Known)
                        .unwrap_or(AddressFact::Unknown),
                    Instruction::Copy { .. } => AddressFact::Pending,
                    Instruction::Primitive {
                        op: NumericOp::I32Add | NumericOp::I32Sub,
                        ..
                    } => AddressFact::Pending,
                    _ => AddressFact::Unknown,
                };
                facts.insert(destination, fact);
            }
        }
    }

    let block_inputs = block_parameter_inputs(function);
    loop {
        let previous = facts.clone();
        for block in &function.blocks {
            for instruction in &block.instructions {
                let Some(destination) = instruction.destination() else {
                    continue;
                };
                let fact = instruction_fact(instruction, &previous, string_offsets);
                facts.insert(destination, fact);
            }
            for parameter in &block.parameters {
                let incoming = block_inputs
                    .get(parameter)
                    .map(|arguments| {
                        arguments
                            .iter()
                            .map(|argument| {
                                previous
                                    .get(argument)
                                    .copied()
                                    .unwrap_or(AddressFact::Unknown)
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                facts.insert(*parameter, join_facts(&incoming));
            }
        }
        if facts == previous {
            break;
        }
    }

    for fact in facts.values_mut() {
        if *fact == AddressFact::Pending {
            *fact = AddressFact::Unknown;
        }
    }
    facts
}

fn block_parameter_inputs(function: &Function) -> HashMap<ValueId, Vec<ValueId>> {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let mut inputs = HashMap::<ValueId, Vec<ValueId>>::new();
    for block in &function.blocks {
        let Some(Terminator::Jump {
            target, arguments, ..
        }) = &block.terminator
        else {
            continue;
        };
        let Some(target_block) = blocks.get(target) else {
            continue;
        };
        for (parameter, argument) in target_block.parameters.iter().zip(arguments) {
            inputs.entry(*parameter).or_default().push(*argument);
        }
    }
    inputs
}

fn instruction_fact(
    instruction: &Instruction,
    facts: &HashMap<ValueId, AddressFact>,
    string_offsets: &HashMap<String, u32>,
) -> AddressFact {
    let fact = |value: ValueId| facts.get(&value).copied().unwrap_or(AddressFact::Unknown);
    match instruction {
        Instruction::Constant { value, .. } => AddressFact::Known(*value as u32),
        Instruction::StringConstant { bytes, .. } => string_offsets
            .get(bytes)
            .copied()
            .map(AddressFact::Known)
            .unwrap_or(AddressFact::Unknown),
        Instruction::Copy { value, .. } => fact(*value),
        Instruction::Primitive {
            op: NumericOp::I32Add,
            left,
            right,
            ..
        } => combine_i32(fact(*left), fact(*right), u32::wrapping_add),
        Instruction::Primitive {
            op: NumericOp::I32Sub,
            left,
            right,
            ..
        } => combine_i32(fact(*left), fact(*right), u32::wrapping_sub),
        Instruction::Call {
            function,
            arguments,
            ..
        } if *function == REALLOC_SYMBOL => arguments
            .get(3)
            .and_then(|length| match fact(*length) {
                AddressFact::Known(size) => Some(AddressFact::Allocated(size)),
                _ => None,
            })
            .unwrap_or(AddressFact::Unknown),
        _ => AddressFact::Unknown,
    }
}

fn combine_i32(
    left: AddressFact,
    right: AddressFact,
    operation: fn(u32, u32) -> u32,
) -> AddressFact {
    match (left, right) {
        (AddressFact::Known(left), AddressFact::Known(right)) => {
            AddressFact::Known(operation(left, right))
        }
        (AddressFact::Unknown, _) | (_, AddressFact::Unknown) => AddressFact::Unknown,
        _ => AddressFact::Pending,
    }
}

fn join_facts(facts: &[AddressFact]) -> AddressFact {
    if facts.is_empty() {
        return AddressFact::Unknown;
    }
    let mut resolved = None;
    let mut pending = false;
    for fact in facts {
        match fact {
            AddressFact::Pending => pending = true,
            AddressFact::Unknown => return AddressFact::Unknown,
            AddressFact::Known(_) | AddressFact::Allocated(_) => match resolved {
                Some(previous) if previous != *fact => return AddressFact::Unknown,
                Some(_) => {}
                None => resolved = Some(*fact),
            },
        }
    }
    if pending {
        AddressFact::Pending
    } else {
        resolved.unwrap_or(AddressFact::Unknown)
    }
}
