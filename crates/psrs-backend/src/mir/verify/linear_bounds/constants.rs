use crate::mir::{Function, Instruction, NumericOp, ValueId, ValueType};
use std::collections::{HashMap, hash_map::Entry};

pub(super) fn constant_values(
    function: &Function,
    incoming: &HashMap<crate::mir::BlockId, Vec<Vec<Option<ValueId>>>>,
) -> HashMap<ValueId, i32> {
    let mut constants = HashMap::new();
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
                let Some(value) = arguments
                    .iter()
                    .map(|argument| constants.get(argument).copied())
                    .collect::<Option<Vec<_>>>()
                    .and_then(|values| join_constants(&values))
                else {
                    continue;
                };
                changed |= insert_if_new(&mut constants, *parameter, value);
            }
        }
        for block in &function.blocks {
            let mut memory_constants = Vec::new();
            for instruction in &block.instructions {
                changed |=
                    update_memory_constants(instruction, &mut constants, &mut memory_constants);
                match instruction {
                    Instruction::Constant {
                        destination, value, ..
                    } => changed |= insert_if_new(&mut constants, *destination, *value),
                    Instruction::Copy {
                        destination, value, ..
                    } => {
                        if let Some(value) = constants.get(value).copied() {
                            changed |= insert_if_new(&mut constants, *destination, value);
                        }
                    }
                    Instruction::Primitive {
                        destination,
                        op,
                        left,
                        right,
                        ..
                    } => {
                        if let (Some(left), Some(right)) =
                            (constants.get(left).copied(), constants.get(right).copied())
                            && let Some(value) = fold_i32(*op, left, right)
                        {
                            changed |= insert_if_new(&mut constants, *destination, value);
                        }
                    }
                    _ => {}
                }
            }
        }
        if !changed {
            return constants;
        }
    }
}

fn join_constants(values: &[i32]) -> Option<i32> {
    let first = *values.first()?;
    values.iter().all(|value| *value == first).then_some(first)
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

fn fold_i32(op: NumericOp, left: i32, right: i32) -> Option<i32> {
    Some(match op {
        NumericOp::I32Add => left.wrapping_add(right),
        NumericOp::I32Sub => left.wrapping_sub(right),
        NumericOp::I32Mul => left.wrapping_mul(right),
        NumericOp::I32And => left & right,
        NumericOp::I32Or => left | right,
        NumericOp::BoolAnd => i32::from(left != 0 && right != 0),
        NumericOp::BoolOr => i32::from(left != 0 || right != 0),
        NumericOp::I32Eq | NumericOp::BoolEq => i32::from(left == right),
        NumericOp::I32Ne | NumericOp::BoolNe => i32::from(left != right),
        NumericOp::I32LtS => i32::from(left < right),
        NumericOp::I32LeS => i32::from(left <= right),
        NumericOp::I32GtS => i32::from(left > right),
        NumericOp::I32GeS => i32::from(left >= right),
        _ => return None,
    })
}

fn update_memory_constants(
    instruction: &Instruction,
    constants: &mut HashMap<ValueId, i32>,
    memory_constants: &mut Vec<(ValueId, u32, u32, ValueType, i32)>,
) -> bool {
    let mut changed = false;
    match instruction {
        Instruction::LinearStore {
            address,
            value,
            offset,
            ty,
            ..
        } => {
            let Some(width) = linear_width(*ty) else {
                memory_constants.clear();
                return false;
            };
            let Some(end) = offset.checked_add(width) else {
                memory_constants.clear();
                return false;
            };
            if memory_constants
                .iter()
                .any(|(stored_address, ..)| stored_address != address)
            {
                memory_constants.clear();
            }
            memory_constants.retain(|(_, stored_start, stored_end, _, _)| {
                *stored_end <= *offset || end <= *stored_start
            });
            if let Some(value) = constants.get(value).copied() {
                memory_constants.push((*address, *offset, end, *ty, value));
            }
        }
        Instruction::LinearLoad {
            destination,
            address,
            offset,
            ty,
            ..
        } => {
            let Some(width) = linear_width(*ty) else {
                return false;
            };
            let Some(end) = offset.checked_add(width) else {
                return false;
            };
            if let Some((_, _, _, _, value)) = memory_constants.iter().rev().find(
                |(stored_address, stored_start, stored_end, stored_type, _)| {
                    stored_address == address
                        && stored_start == offset
                        && stored_end == &end
                        && stored_type == ty
                },
            ) {
                changed |= insert_if_new(constants, *destination, *value);
            }
        }
        Instruction::LinearMemoryCopy { .. }
        | Instruction::Store { .. }
        | Instruction::Call { .. }
        | Instruction::CallVoid { .. }
        | Instruction::CallRef { .. }
        | Instruction::ClosureCall { .. }
        | Instruction::LinearClosureCall { .. } => memory_constants.clear(),
        _ => {}
    }
    changed
}

fn linear_width(ty: ValueType) -> Option<u32> {
    match ty {
        ValueType::I32 | ValueType::Boolean => Some(4),
        ValueType::F64 => Some(8),
        _ => None,
    }
}
