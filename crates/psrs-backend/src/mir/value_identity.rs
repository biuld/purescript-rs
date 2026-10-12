//! Shared reference identity and loop parameter views for MIR consumers.
use super::{BlockId, Function, Instruction, Terminator};
use crate::types::ValueId;
use std::collections::{HashMap, HashSet};

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

/// Incoming values of a non-loop SSA join parameter. Unknown edges are not
/// treated as a valid empty provenance set.
pub(crate) fn incoming_values(function: &Function, value: ValueId) -> Option<Vec<ValueId>> {
    let (block, index) = function.blocks.iter().find_map(|block| {
        block
            .parameters
            .iter()
            .position(|parameter| *parameter == value)
            .map(|index| (block, index))
    })?;
    if Some(block.id) == loop_header(function) {
        return None;
    }
    let mut incoming = Vec::new();
    for predecessor in &function.blocks {
        let Some(terminator) = &predecessor.terminator else {
            continue;
        };
        if super::cfg::successors(terminator).contains(&block.id) {
            let Terminator::Jump {
                target, arguments, ..
            } = terminator
            else {
                return None;
            };
            if *target != block.id {
                return None;
            }
            incoming.push(*arguments.get(index)?);
        }
    }
    (!incoming.is_empty()).then_some(incoming)
}

pub(crate) type Point = (BlockId, usize);

pub(crate) fn precedes(
    source: Point,
    target: Point,
    dominators: &HashMap<crate::mir::BlockId, HashSet<crate::mir::BlockId>>,
) -> bool {
    if source.0 == target.0 {
        source.1 < target.1
    } else {
        dominators
            .get(&target.0)
            .is_some_and(|set| set.contains(&source.0))
    }
}

/// Prove that a producer dominates its consumer and executes inside the current
/// loop iteration, excluding allocations reused from the entry preheader.
/// Callers separately prove that the producer is the required allocation.
pub(crate) fn fresh_iteration_allocation(
    function: &Function,
    allocation: ValueId,
    consumer: Point,
    points: &HashMap<ValueId, Point>,
    dominators: &HashMap<crate::mir::BlockId, HashSet<crate::mir::BlockId>>,
) -> bool {
    let Some(point) = points.get(&allocation).copied() else {
        return false;
    };
    precedes(point, consumer, dominators)
        && loop_header(function).is_none_or(|header| {
            dominators
                .get(&point.0)
                .is_some_and(|set| set.contains(&header))
        })
}
