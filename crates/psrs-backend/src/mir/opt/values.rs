//! SSA copy forwarding and dead pure-instruction elimination.

use super::effects;
use crate::mir::{Function, Instruction, Module, Terminator};
use crate::types::ValueId;
use std::collections::{HashMap, HashSet};

pub(super) fn forward_copies(module: &mut Module) {
    for function in &mut module.functions {
        forward_function(function);
    }
}

fn forward_function(function: &mut Function) {
    let aliases = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            Instruction::Copy {
                destination, value, ..
            } => Some((*destination, *value)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    if aliases.is_empty() {
        return;
    }
    let aliases = aliases
        .keys()
        .map(|destination| (*destination, resolve_alias(*destination, &aliases)))
        .collect::<HashMap<_, _>>();
    function.result = resolve_alias(function.result, &aliases);

    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            remap_instruction(instruction, &aliases);
        }
        if let Some(terminator) = &mut block.terminator {
            remap_terminator(terminator, &aliases);
        }
        block
            .instructions
            .retain(|instruction| !matches!(instruction, Instruction::Copy { .. }));
    }
}

pub(super) fn eliminate_dead_pure(module: &mut Module) {
    for function in &mut module.functions {
        eliminate_function(function);
    }
}

fn eliminate_function(function: &mut Function) {
    let mut definitions = HashMap::<ValueId, (usize, usize)>::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if let Some(destination) = instruction.destination() {
                definitions.insert(destination, (block_index, instruction_index));
            }
        }
    }

    let mut live_values = Vec::new();
    let mut live_instructions = HashSet::new();
    live_values.push(function.result);
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if !effects::is_pure_and_total(instruction) {
                live_instructions.insert((block_index, instruction_index));
                live_values.extend(instruction.operands());
            }
        }
        if let Some(terminator) = &block.terminator {
            live_values.extend(terminator_operands(terminator));
        }
    }

    while let Some(value) = live_values.pop() {
        let Some(&(block_index, instruction_index)) = definitions.get(&value) else {
            continue;
        };
        if !live_instructions.insert((block_index, instruction_index)) {
            continue;
        }
        live_values.extend(function.blocks[block_index].instructions[instruction_index].operands());
    }

    for (block_index, block) in function.blocks.iter_mut().enumerate() {
        let mut instruction_index = 0;
        block.instructions.retain(|_| {
            let keep = live_instructions.contains(&(block_index, instruction_index));
            instruction_index += 1;
            keep
        });
    }
}

fn terminator_operands(terminator: &Terminator) -> Vec<ValueId> {
    match terminator {
        Terminator::Return { value, .. } => vec![*value],
        Terminator::Jump { arguments, .. } => arguments.clone(),
        Terminator::Branch { condition, .. } => vec![*condition],
        Terminator::Switch { value, .. } => vec![*value],
        Terminator::ReturnCall { arguments, .. } => arguments.clone(),
        Terminator::ReturnCallRef {
            function,
            arguments,
            ..
        } => std::iter::once(*function)
            .chain(arguments.iter().copied())
            .collect(),
    }
}

pub(crate) fn remap_instruction(
    instruction: &mut Instruction,
    mapping: &HashMap<ValueId, ValueId>,
) {
    use Instruction as I;
    let replace = |value: &mut ValueId| {
        *value = mapping.get(value).copied().unwrap_or(*value);
    };
    match instruction {
        I::Copy {
            destination, value, ..
        } => {
            replace(destination);
            replace(value);
        }
        I::Constant { destination, .. }
        | I::NumberConstant { destination, .. }
        | I::ArrayNewData { destination, .. }
        | I::RefNull { destination, .. }
        | I::Unreachable { destination, .. } => replace(destination),
        I::Primitive {
            destination,
            left,
            right,
            ..
        } => {
            replace(destination);
            replace(left);
            replace(right);
        }
        I::UnaryPrimitive {
            destination, value, ..
        }
        | I::RefIsNull {
            destination, value, ..
        }
        | I::RefTest {
            destination, value, ..
        }
        | I::RefCast {
            destination, value, ..
        }
        | I::I31New {
            destination, value, ..
        }
        | I::I31GetS {
            destination, value, ..
        }
        | I::ArrayClone {
            destination, value, ..
        }
        | I::ArrayLen {
            destination, value, ..
        }
        | I::WrapI64 {
            destination, value, ..
        }
        | I::WidenI64 {
            destination, value, ..
        } => {
            replace(destination);
            replace(value);
        }
        I::Call {
            destination,
            arguments,
            ..
        } => {
            replace(destination);
            arguments.iter_mut().for_each(replace);
        }
        I::RefFunc { destination, .. } => replace(destination),
        I::ClosureNew {
            destination,
            captures,
            ..
        }
        | I::ArrayNew {
            destination,
            elements: captures,
            ..
        }
        | I::StructNew {
            destination,
            arguments: captures,
            ..
        } => {
            replace(destination);
            captures.iter_mut().for_each(replace);
        }
        I::ArrayNewDefault {
            destination,
            length,
            source,
            ..
        } => {
            replace(destination);
            replace(length);
            replace(source);
        }
        I::CallRef {
            destination,
            function,
            arguments,
            ..
        }
        | I::ClosureCall {
            destination,
            function,
            arguments,
            ..
        } => {
            replace(destination);
            replace(function);
            arguments.iter_mut().for_each(replace);
        }
        I::ClosureGetCapture {
            destination,
            closure,
            ..
        } => {
            replace(destination);
            replace(closure);
        }
        I::CallVoid { arguments, .. } => arguments.iter_mut().for_each(replace),
        I::StructGet {
            destination, value, ..
        } => {
            replace(destination);
            replace(value);
        }
        I::StructSet {
            value, new_value, ..
        } => {
            replace(value);
            replace(new_value);
        }
        I::ArrayGet {
            destination,
            value,
            index,
            ..
        } => {
            replace(destination);
            replace(value);
            replace(index);
        }
        I::ArraySet {
            value,
            index,
            new_value,
            ..
        } => {
            replace(value);
            replace(index);
            replace(new_value);
        }
        I::Load {
            destination,
            address,
            ..
        }
        | I::Load8U {
            destination,
            address,
            ..
        } => {
            replace(destination);
            replace(address);
        }
        I::Store { address, value, .. }
        | I::Store8 { address, value, .. }
        | I::Store16 { address, value, .. }
        | I::StoreI64 { address, value, .. }
        | I::StoreF32 { address, value, .. }
        | I::StoreF64 { address, value, .. } => {
            replace(address);
            replace(value);
        }
        I::TrapIf { condition, .. } => replace(condition),
    }
}

pub(crate) fn remap_terminator(terminator: &mut Terminator, mapping: &HashMap<ValueId, ValueId>) {
    let replace = |value: &mut ValueId| {
        *value = mapping.get(value).copied().unwrap_or(*value);
    };
    match terminator {
        Terminator::Return { value, .. } => replace(value),
        Terminator::Jump { arguments, .. } => arguments.iter_mut().for_each(replace),
        Terminator::Branch { condition, .. } => replace(condition),
        Terminator::Switch { value, .. } => replace(value),
        Terminator::ReturnCall { arguments, .. } => arguments.iter_mut().for_each(replace),
        Terminator::ReturnCallRef {
            function,
            arguments,
            ..
        } => {
            replace(function);
            arguments.iter_mut().for_each(replace);
        }
    }
}

fn resolve_alias(mut value: ValueId, mapping: &HashMap<ValueId, ValueId>) -> ValueId {
    let mut visited = HashSet::new();
    while visited.insert(value)
        && let Some(next) = mapping.get(&value).copied()
        && next != value
    {
        value = next;
    }
    value
}
