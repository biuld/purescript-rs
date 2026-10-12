//! Shared reference identity and loop parameter views for MIR consumers.
use super::{BlockId, Function, Instruction, Terminator};
use crate::types::ValueId;
use std::collections::HashMap;

pub(crate) type Definitions<'a> = HashMap<ValueId, &'a Instruction>;

pub(crate) fn definitions(function: &Function) -> Definitions<'_> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| {
            instruction
                .destination()
                .map(|destination| (destination, instruction))
        })
        .collect()
}

pub(crate) fn reference_origin(mut value: ValueId, definitions: &Definitions<'_>) -> ValueId {
    for _ in 0..definitions.len() {
        match definitions.get(&value).copied() {
            Some(
                Instruction::Copy { value: source, .. }
                | Instruction::RefCast { value: source, .. },
            ) => value = *source,
            _ => break,
        }
    }
    value
}

pub(crate) fn loop_header(function: &Function) -> Option<BlockId> {
    function
        .blocks
        .iter()
        .find(|block| block.id == function.entry)
        .and_then(|block| match &block.terminator {
            Some(Terminator::Jump {
                target, arguments, ..
            }) if arguments == &function.parameters => Some(*target),
            _ => None,
        })
}

pub(crate) fn loop_parameters(function: &Function) -> &[ValueId] {
    let header = loop_header(function);
    function
        .blocks
        .iter()
        .find(|block| Some(block.id) == header)
        .map_or(&function.parameters, |block| &block.parameters)
}
