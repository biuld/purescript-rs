#![allow(dead_code)]
//! Shared reference identity and loop parameter views for MIR consumers.
use super::{BlockId, Function, Instruction, Terminator};
use crate::types::ValueId;
use std::collections::{HashMap, HashSet};

pub(crate) struct Definitions<'a> {
    instructions: HashMap<ValueId, &'a Instruction>,
    joins: HashMap<ValueId, Vec<ValueId>>,
}

impl<'a> std::ops::Deref for Definitions<'a> {
    type Target = HashMap<ValueId, &'a Instruction>;
    fn deref(&self) -> &Self::Target {
        &self.instructions
    }
}

pub(crate) fn definitions(function: &Function) -> Definitions<'_> {
    index(function, None)
}

/// Restrict join inputs to paths from the semantic step being proved. The
/// executor loop header starts the next step and is not part of this result.
pub(crate) fn step_definitions(function: &Function, start: BlockId) -> Definitions<'_> {
    let reachable = super::cfg::reachable_from(function, start, loop_header(function));
    index(function, Some(&reachable))
}

fn index<'a>(function: &'a Function, reachable: Option<&HashSet<BlockId>>) -> Definitions<'a> {
    Definitions {
        instructions: function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter_map(|instruction| instruction.destination().map(|value| (value, instruction)))
            .collect(),
        joins: function
            .blocks
            .iter()
            .flat_map(|block| &block.parameters)
            .filter_map(|value| {
                incoming_from(function, *value, reachable).map(|inputs| (*value, inputs))
            })
            .collect(),
    }
}

/// Copies, casts and unanimous non-loop joins preserve reference identity.
/// Loop parameters remain distinct: their incoming values represent different
/// machine iterations, rather than aliases of one execution result.
pub(crate) fn reference_origin(value: ValueId, definitions: &Definitions<'_>) -> ValueId {
    fn resolve(
        value: ValueId,
        definitions: &Definitions<'_>,
        active: &mut HashSet<ValueId>,
    ) -> ValueId {
        if !active.insert(value) {
            return value;
        }
        let origin = match definitions.get(&value).copied() {
            Some(Instruction::Copy { value, .. } | Instruction::RefCast { value, .. }) => {
                resolve(*value, definitions, active)
            }
            _ => {
                if let Some(inputs) = definitions.joins.get(&value) {
                    let mut origins = inputs
                        .iter()
                        .map(|input| resolve(*input, definitions, active));
                    let first = origins.next().unwrap_or(value);
                    if origins.all(|origin| origin == first) {
                        first
                    } else {
                        value
                    }
                } else {
                    value
                }
            }
        };
        active.remove(&value);
        origin
    }
    resolve(value, definitions, &mut HashSet::new())
}

/// Resolve a result along one concrete CFG path, retaining its incoming SSA
/// arguments instead of combining values from mutually exclusive exits.
pub(crate) fn path_origin(
    mut value: ValueId,
    definitions: &Definitions<'_>,
    aliases: &HashMap<ValueId, ValueId>,
) -> ValueId {
    let mut seen = HashSet::new();
    while seen.insert(value) {
        let next =
            aliases
                .get(&value)
                .copied()
                .or_else(|| match definitions.get(&value).copied() {
                    Some(Instruction::Copy { value, .. } | Instruction::RefCast { value, .. }) => {
                        Some(*value)
                    }
                    _ => None,
                });
        match next {
            Some(next) => value = next,
            None => break,
        }
    }
    value
}

/// Integer evidence through copies and unanimous normal joins. Unknown values
/// and cyclic joins cannot establish a nominal constructor's physical tag.
pub(crate) fn integer_constant(value: ValueId, definitions: &Definitions<'_>) -> Option<i32> {
    fn resolve(
        value: ValueId,
        definitions: &Definitions<'_>,
        active: &mut HashSet<ValueId>,
    ) -> Option<i32> {
        if !active.insert(value) {
            return None;
        }
        let constant = match definitions.get(&value).copied() {
            Some(Instruction::Constant { value, .. }) => Some(*value),
            Some(Instruction::Copy { value, .. }) => resolve(*value, definitions, active),
            _ => definitions.joins.get(&value).and_then(|inputs| {
                let first = resolve(*inputs.first()?, definitions, active)?;
                inputs
                    .iter()
                    .skip(1)
                    .all(|input| resolve(*input, definitions, active) == Some(first))
                    .then_some(first)
            }),
        };
        active.remove(&value);
        constant
    }
    resolve(value, definitions, &mut HashSet::new())
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
    incoming_from(function, value, None)
}

fn incoming_from(
    function: &Function,
    value: ValueId,
    reachable: Option<&HashSet<BlockId>>,
) -> Option<Vec<ValueId>> {
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
        if reachable.is_some_and(|blocks| !blocks.contains(&predecessor.id)) {
            continue;
        }
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
