use crate::mir::{Function, Instruction, NumericOp, Terminator, ValueId};
use constants::constant_values;
use int_range::{IntegerRange, arithmetic_range, comparison_range, refine_comparison_false};
use range_flow::{block_entry_ranges, comparison_operands};
use std::collections::{HashMap, hash_map::Entry};

mod constants;
mod int_range;
mod range_flow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LinearPointerBounds {
    bytes_before: u32,
    bytes_after: u32,
    invalid: bool,
}

/// Tracks local allocation bounds through copies, constant pointer arithmetic,
/// and block parameters whose incoming pointers all have known bounds. It folds
/// constant stores, loads, and integer comparisons so a provably trapping guard
/// makes its following instructions unreachable to this analysis. Guarded
/// dynamic offsets flow across acyclic CFG edges; loop-carried ranges and
/// incoming pointers without local provenance remain unknown.
pub(super) fn linear_allocation_bounds(function: &Function) -> HashMap<ValueId, u32> {
    let incoming = incoming_block_arguments(function);
    let constants = constant_values(function, &incoming);
    let range_entries = block_entry_ranges(function, &constants);
    let comparisons = comparison_operands(function);
    let mut origins = HashMap::new();
    loop {
        let mut changed = false;
        for block in &function.blocks {
            let Some(edges) = incoming.get(&block.id) else {
                continue;
            };
            for (index, parameter) in block.parameters.iter().enumerate() {
                let arguments = edges
                    .iter()
                    .filter_map(|edge| edge.get(index).copied().flatten())
                    .collect::<Vec<_>>();
                if arguments.len() != edges.len() || arguments.is_empty() {
                    continue;
                }
                if let Some(bounds) = arguments
                    .iter()
                    .map(|argument| origins.get(argument).copied())
                    .collect::<Option<Vec<_>>>()
                    .map(|bounds| join_bounds(&bounds))
                {
                    changed |= insert_if_new(&mut origins, *parameter, bounds);
                }
            }
        }
        for block in &function.blocks {
            let mut ranges = function
                .values
                .iter()
                .filter_map(|value| IntegerRange::for_type(value.ty).map(|range| (value.id, range)))
                .collect::<HashMap<_, _>>();
            for (value, constant) in &constants {
                ranges.insert(*value, IntegerRange::exact(*constant));
            }
            if let Some(entry) = range_entries.get(&block.id) {
                ranges.extend(entry.iter().map(|(value, range)| (*value, *range)));
            }
            for instruction in &block.instructions {
                if let Instruction::TrapIf { condition, .. } = instruction {
                    let condition_range = ranges.get(condition).copied();
                    if constants
                        .get(condition)
                        .is_some_and(|condition| *condition != 0)
                        || condition_range
                            .and_then(IntegerRange::exact_value)
                            .is_some_and(|condition| condition != 0)
                    {
                        break;
                    }
                    if constants.get(condition) != Some(&0)
                        && condition_range.and_then(IntegerRange::exact_value) != Some(0)
                        && let Some((operation, left, right)) = comparisons.get(condition)
                    {
                        refine_comparison_false(*operation, *left, *right, &mut ranges);
                    }
                }
                match instruction {
                    Instruction::Constant {
                        destination, value, ..
                    } => {
                        ranges.insert(*destination, IntegerRange::exact(*value));
                    }
                    Instruction::LinearAlloc {
                        destination, bytes, ..
                    }
                    | Instruction::LinearClosureNew {
                        destination,
                        allocation_bytes: bytes,
                        ..
                    } => {
                        changed |= insert_if_new(
                            &mut origins,
                            *destination,
                            LinearPointerBounds {
                                bytes_before: 0,
                                bytes_after: *bytes,
                                invalid: false,
                            },
                        );
                        ranges.remove(destination);
                    }
                    Instruction::LinearAllocDynamic {
                        destination, bytes, ..
                    } => {
                        if let Some(bytes) =
                            constants.get(bytes).copied().filter(|bytes| *bytes > 0)
                        {
                            changed |= insert_if_new(
                                &mut origins,
                                *destination,
                                LinearPointerBounds {
                                    bytes_before: 0,
                                    bytes_after: bytes as u32,
                                    invalid: false,
                                },
                            );
                        }
                        ranges.remove(destination);
                    }
                    Instruction::Copy {
                        destination, value, ..
                    } => {
                        if let Some(origin) = origins.get(value).copied() {
                            changed |= insert_if_new(&mut origins, *destination, origin);
                        }
                        if origins.contains_key(destination) {
                            ranges.remove(destination);
                        } else if let Some(range) = ranges.get(value).copied() {
                            ranges.insert(*destination, range);
                        }
                    }
                    Instruction::Primitive {
                        destination,
                        op,
                        left,
                        right,
                        ..
                    } => {
                        let left_range = ranges.get(left).copied();
                        let right_range = ranges.get(right).copied();
                        let comparison = matches!(
                            op,
                            NumericOp::I32Eq
                                | NumericOp::I32Ne
                                | NumericOp::I32LtS
                                | NumericOp::I32LeS
                                | NumericOp::I32GtS
                                | NumericOp::I32GeS
                                | NumericOp::BoolEq
                                | NumericOp::BoolNe
                        );
                        let mut pointer_bounds = None;
                        if matches!(op, NumericOp::I32Add | NumericOp::I32Sub)
                            && let Some(origin) = origins.get(left).copied()
                            && !origins.contains_key(right)
                            && let Some(offset) = right_range
                        {
                            let (minimum, maximum) = if *op == NumericOp::I32Sub {
                                (-offset.maximum, -offset.minimum)
                            } else {
                                (offset.minimum, offset.maximum)
                            };
                            pointer_bounds = offset_bounds_range(origin, minimum, maximum);
                        } else if *op == NumericOp::I32Add
                            && let Some(origin) = origins.get(right).copied()
                            && !origins.contains_key(left)
                            && let Some(offset) = left_range
                        {
                            pointer_bounds =
                                offset_bounds_range(origin, offset.minimum, offset.maximum);
                        }
                        if let Some(bounds) = pointer_bounds {
                            changed |= insert_if_new(&mut origins, *destination, bounds);
                            ranges.remove(destination);
                        } else if comparison {
                            if let Some((left, right)) = left_range.zip(right_range)
                                && let Some(range) = comparison_range(*op, left, right)
                            {
                                ranges.insert(*destination, range);
                            }
                        } else if let Some((left, right)) = left_range.zip(right_range)
                            && let Some(range) = arithmetic_range(*op, left, right)
                        {
                            ranges.insert(*destination, range);
                        }
                    }
                    Instruction::LinearLoad { destination, .. } => {
                        if let Some(constant) = constants.get(destination).copied() {
                            ranges.insert(*destination, IntegerRange::exact(constant));
                        }
                    }
                    _ => {}
                }
            }
        }
        if !changed {
            break;
        }
    }
    origins
        .into_iter()
        .map(|(value, bounds)| {
            (
                value,
                if bounds.invalid {
                    0
                } else {
                    bounds.bytes_after
                },
            )
        })
        .collect()
}

fn incoming_block_arguments(
    function: &Function,
) -> HashMap<crate::mir::BlockId, Vec<Vec<Option<ValueId>>>> {
    let mut incoming = HashMap::<_, Vec<Vec<Option<ValueId>>>>::new();
    for block in &function.blocks {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        match terminator {
            Terminator::Jump {
                target, arguments, ..
            } => incoming
                .entry(*target)
                .or_default()
                .push(arguments.iter().copied().map(Some).collect()),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                incoming.entry(*then_block).or_default().push(Vec::new());
                incoming.entry(*else_block).or_default().push(Vec::new());
            }
            Terminator::Return { .. } => {}
        }
    }
    incoming
}

fn join_bounds(bounds: &[LinearPointerBounds]) -> LinearPointerBounds {
    let invalid = bounds.iter().any(|bounds| bounds.invalid);
    LinearPointerBounds {
        bytes_before: bounds
            .iter()
            .map(|bounds| bounds.bytes_before)
            .min()
            .expect("pointer bounds join is nonempty"),
        bytes_after: if invalid {
            0
        } else {
            bounds
                .iter()
                .map(|bounds| bounds.bytes_after)
                .min()
                .expect("pointer bounds join is nonempty")
        },
        invalid,
    }
}

fn insert_if_new<K: Eq + std::hash::Hash, V: Copy + PartialEq>(
    map: &mut HashMap<K, V>,
    key: K,
    value: V,
) -> bool {
    if let Entry::Vacant(entry) = map.entry(key) {
        entry.insert(value);
        true
    } else {
        false
    }
}

fn offset_bounds_range(
    bounds: LinearPointerBounds,
    minimum_offset: i64,
    maximum_offset: i64,
) -> Option<LinearPointerBounds> {
    if bounds.invalid {
        return Some(bounds);
    }
    if minimum_offset > maximum_offset {
        return None;
    }
    let bytes_before = bounds.bytes_before as i64 + minimum_offset;
    let bytes_after = bounds.bytes_after as i64 - maximum_offset;
    if bytes_before < 0 || bytes_after < 0 {
        return Some(invalid_bounds());
    }
    Some(LinearPointerBounds {
        bytes_before: u32::try_from(bytes_before).ok()?,
        bytes_after: u32::try_from(bytes_after).ok()?,
        invalid: false,
    })
}

fn invalid_bounds() -> LinearPointerBounds {
    LinearPointerBounds {
        bytes_before: 0,
        bytes_after: 0,
        invalid: true,
    }
}

#[cfg(test)]
mod tests;
