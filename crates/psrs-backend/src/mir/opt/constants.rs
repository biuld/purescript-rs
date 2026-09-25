//! Scalar propagation and folding of proven-safe operations.

use crate::mir::{Function, Instruction, Module, NumericOp, Terminator, UnaryOp};
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScalarConstant {
    I32(i32),
    Boolean(bool),
    F32(u32),
    F64(u64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Fact {
    #[default]
    Unknown,
    Constant(ScalarConstant),
    Overdefined,
}

pub(super) fn propagate(module: &mut Module) -> bool {
    let mut changed = false;
    for function in &mut module.functions {
        changed |= propagate_function(function);
    }
    changed
}

pub(super) fn boolean_constant(function: &Function, value: ValueId) -> Option<bool> {
    match analyze(function).get(&value).copied().unwrap_or_default() {
        Fact::Constant(ScalarConstant::Boolean(value)) => Some(value),
        _ => None,
    }
}

pub(super) fn integer_constant(function: &Function, value: ValueId) -> Option<i32> {
    match analyze(function).get(&value).copied().unwrap_or_default() {
        Fact::Constant(ScalarConstant::I32(value)) => Some(value),
        _ => None,
    }
}

fn propagate_function(function: &mut Function) -> bool {
    let facts = analyze(function);
    let mut changed = materialize_block_parameters(function, &facts);
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            let Some(destination) = instruction.destination() else {
                continue;
            };
            if !matches!(
                instruction,
                Instruction::Copy { .. }
                    | Instruction::Primitive { .. }
                    | Instruction::UnaryPrimitive { .. }
            ) {
                continue;
            }
            let Some(Fact::Constant(value)) = facts.get(&destination).copied() else {
                continue;
            };
            let span = instruction.span();
            if let Some(constant) = value.instruction(destination, span) {
                *instruction = constant;
                changed = true;
            }
        }
    }
    changed
}

fn analyze(function: &Function) -> HashMap<ValueId, Fact> {
    let types = function
        .values
        .iter()
        .map(|value| (value.id, value.ty))
        .collect::<HashMap<_, _>>();
    let reachable = super::cfg::reachable_blocks(function.entry, &function.blocks);
    let predecessors = incoming_values(function, &reachable);
    let mut facts = function
        .values
        .iter()
        .map(|value| (value.id, Fact::Unknown))
        .collect::<HashMap<_, _>>();
    for parameter in &function.parameters {
        facts.insert(*parameter, Fact::Overdefined);
    }

    // The lattice has at most two state changes per value. Repeating to a
    // fixed point lets constants flow through block parameters regardless of
    // block layout order, while loop-carried unknowns stay conservative.
    let limit = function.values.len().saturating_mul(2).saturating_add(4);
    for _ in 0..limit {
        let mut changed = false;
        for block in &function.blocks {
            if !reachable.contains(&block.id) {
                continue;
            }
            for (index, parameter) in block.parameters.iter().enumerate() {
                let candidate = merge_incoming(predecessors.get(&(block.id, index)), &facts);
                changed |= update_fact(&mut facts, *parameter, candidate);
            }
            for instruction in &block.instructions {
                let Some(destination) = instruction.destination() else {
                    continue;
                };
                let candidate = instruction_fact(instruction, &facts, &types);
                changed |= update_fact(&mut facts, destination, candidate);
            }
        }
        if !changed {
            break;
        }
    }
    facts
}

type Incoming = HashMap<(crate::mir::BlockId, usize), Vec<Option<ValueId>>>;

fn incoming_values(function: &Function, reachable: &HashSet<crate::mir::BlockId>) -> Incoming {
    let mut incoming = HashMap::new();
    for block in &function.blocks {
        if !reachable.contains(&block.id) {
            continue;
        }
        let Some(terminator) = &block.terminator else {
            continue;
        };
        match terminator {
            Terminator::Jump {
                target, arguments, ..
            } => {
                if let Some(target_block) = function.blocks.iter().find(|b| b.id == *target) {
                    for (index, _) in target_block.parameters.iter().enumerate() {
                        incoming
                            .entry((*target, index))
                            .or_insert_with(Vec::new)
                            .push(arguments.get(index).copied());
                    }
                }
            }
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                for target in [then_block, else_block] {
                    if let Some(target_block) = function.blocks.iter().find(|b| b.id == *target) {
                        for (index, _) in target_block.parameters.iter().enumerate() {
                            incoming
                                .entry((*target, index))
                                .or_insert_with(Vec::new)
                                .push(None);
                        }
                    }
                }
            }
            // Switch successors cannot carry block parameters, so there are no
            // incoming values to add to the block-parameter lattice. A tail
            // call passes its arguments to a callee, not to a block.
            Terminator::Switch { .. }
            | Terminator::ReturnCall { .. }
            | Terminator::ReturnCallRef { .. } => {}
            Terminator::Return { .. } => {}
        }
    }
    incoming
}

fn merge_incoming(incoming: Option<&Vec<Option<ValueId>>>, facts: &HashMap<ValueId, Fact>) -> Fact {
    let Some(incoming) = incoming.filter(|values| !values.is_empty()) else {
        return Fact::Overdefined;
    };
    let mut merged = None;
    for value in incoming {
        let candidate = value
            .and_then(|value| facts.get(&value).copied())
            .unwrap_or(Fact::Overdefined);
        match candidate {
            Fact::Unknown => return Fact::Unknown,
            Fact::Overdefined => return Fact::Overdefined,
            Fact::Constant(value) => match merged {
                None => merged = Some(value),
                Some(previous) if previous == value => {}
                Some(_) => return Fact::Overdefined,
            },
        }
    }
    merged.map_or(Fact::Overdefined, Fact::Constant)
}

fn update_fact(facts: &mut HashMap<ValueId, Fact>, value: ValueId, candidate: Fact) -> bool {
    let current = facts.get(&value).copied().unwrap_or(Fact::Overdefined);
    let next = match (current, candidate) {
        (Fact::Unknown, candidate) => candidate,
        (Fact::Constant(previous), Fact::Constant(next)) if previous == next => current,
        (Fact::Constant(_), Fact::Unknown) => current,
        (Fact::Constant(_), _) => Fact::Overdefined,
        (Fact::Overdefined, _) => Fact::Overdefined,
    };
    if current == next {
        false
    } else {
        facts.insert(value, next);
        true
    }
}

fn instruction_fact(
    instruction: &Instruction,
    facts: &HashMap<ValueId, Fact>,
    types: &HashMap<ValueId, ValueType>,
) -> Fact {
    use Instruction as I;
    match instruction {
        I::Constant {
            destination, value, ..
        } => match types.get(destination) {
            Some(ValueType::Boolean) => Fact::Constant(ScalarConstant::Boolean(*value != 0)),
            Some(ValueType::I32) => Fact::Constant(ScalarConstant::I32(*value)),
            _ => Fact::Overdefined,
        },
        I::NumberConstant {
            value, destination, ..
        } => match value.parse::<f64>() {
            Ok(value) if types.get(destination) == Some(&ValueType::F64) => {
                Fact::Constant(ScalarConstant::F64(value.to_bits()))
            }
            _ => Fact::Overdefined,
        },
        I::Copy { value, .. } => facts.get(value).copied().unwrap_or_default(),
        I::Primitive {
            op, left, right, ..
        } => {
            let left = facts.get(left).copied().unwrap_or_default();
            let right = facts.get(right).copied().unwrap_or_default();
            match (left, right) {
                (Fact::Constant(left), Fact::Constant(right)) => {
                    fold_primitive(*op, left, right).map_or(Fact::Overdefined, Fact::Constant)
                }
                (Fact::Overdefined, _) | (_, Fact::Overdefined) => Fact::Overdefined,
                _ => Fact::Unknown,
            }
        }
        I::UnaryPrimitive { op, value, .. } => {
            match facts.get(value).copied().unwrap_or_default() {
                Fact::Constant(value) => {
                    fold_unary(*op, value).map_or(Fact::Overdefined, Fact::Constant)
                }
                Fact::Overdefined => Fact::Overdefined,
                Fact::Unknown => Fact::Unknown,
            }
        }
        _ => Fact::Overdefined,
    }
}

fn materialize_block_parameters(function: &mut Function, facts: &HashMap<ValueId, Fact>) -> bool {
    let mut changed = false;
    // A branch or switch join has a one-value contract the structurer enforces
    // when it derives the join from the CFG edges. Materializing that parameter
    // away would leave the join parameterless, so keep those parameters.
    let join_blocks = crate::mir::cfg::join_blocks(function);
    let mut removals = HashMap::<crate::mir::BlockId, Vec<(usize, Instruction)>>::new();
    for block in &function.blocks {
        if join_blocks.contains(&block.id) {
            continue;
        }
        let span = block
            .instructions
            .first()
            .map(Instruction::span)
            .unwrap_or(function.span);
        for (index, parameter) in block.parameters.iter().enumerate() {
            if let Some(Fact::Constant(constant)) = facts.get(parameter).copied()
                && let Some(instruction) = constant.instruction(*parameter, span)
            {
                removals
                    .entry(block.id)
                    .or_default()
                    .push((index, instruction));
            }
        }
    }
    if removals.is_empty() {
        return false;
    }

    for block in &mut function.blocks {
        let Some(remove) = removals.get_mut(&block.id) else {
            continue;
        };
        remove.sort_by_key(|(index, _)| *index);
        let indices = remove
            .iter()
            .map(|(index, _)| *index)
            .collect::<HashSet<_>>();
        block.parameters = block
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, parameter)| (!indices.contains(&index)).then_some(*parameter))
            .collect();
        let constants = remove
            .iter()
            .map(|(_, instruction)| instruction.clone())
            .collect::<Vec<_>>();
        block.instructions.splice(0..0, constants);
        changed = true;
    }

    for predecessor in &mut function.blocks {
        let Some(Terminator::Jump {
            target, arguments, ..
        }) = predecessor.terminator.as_mut()
        else {
            continue;
        };
        let Some(remove) = removals.get(target) else {
            continue;
        };
        let mut indices = remove.iter().map(|(index, _)| *index).collect::<Vec<_>>();
        indices.sort_unstable_by(|left, right| right.cmp(left));
        for index in indices {
            if index < arguments.len() {
                arguments.remove(index);
            }
        }
    }
    changed
}

impl ScalarConstant {
    fn instruction(self, destination: ValueId, span: TextRange) -> Option<Instruction> {
        Some(match self {
            Self::I32(value) => Instruction::Constant {
                destination,
                value,
                span,
            },
            Self::Boolean(value) => Instruction::Constant {
                destination,
                value: i32::from(value),
                span,
            },
            Self::F32(_) => return None,
            Self::F64(bits) => {
                let value = f64::from_bits(bits);
                if value.is_nan() {
                    return None;
                }
                Instruction::NumberConstant {
                    destination,
                    value: value.to_string(),
                    span,
                }
            }
        })
    }
}

fn fold_primitive(
    op: NumericOp,
    left: ScalarConstant,
    right: ScalarConstant,
) -> Option<ScalarConstant> {
    use NumericOp as N;
    use ScalarConstant as C;
    let compare_i32 = |result| Some(C::Boolean(result));
    let compare_f64 = |result| Some(C::Boolean(result));
    match (op, left, right) {
        (N::I32Add, C::I32(a), C::I32(b)) => Some(C::I32(a.wrapping_add(b))),
        (N::I32Sub, C::I32(a), C::I32(b)) => Some(C::I32(a.wrapping_sub(b))),
        (N::I32Mul, C::I32(a), C::I32(b)) => Some(C::I32(a.wrapping_mul(b))),
        (N::I32DivS, C::I32(a), C::I32(b)) => a.checked_div(b).map(C::I32),
        (N::I32RemS, C::I32(a), C::I32(b)) => (b != 0).then(|| C::I32(a.wrapping_rem(b))),
        (N::I32And, C::I32(a), C::I32(b)) => Some(C::I32(a & b)),
        (N::I32Or, C::I32(a), C::I32(b)) => Some(C::I32(a | b)),
        (N::I32Xor, C::I32(a), C::I32(b)) => Some(C::I32(a ^ b)),
        (N::I32Shl, C::I32(a), C::I32(b)) => Some(C::I32(a.wrapping_shl(b as u32))),
        (N::I32ShrS, C::I32(a), C::I32(b)) => Some(C::I32(a.wrapping_shr(b as u32))),
        (N::I32ShrU, C::I32(a), C::I32(b)) => {
            Some(C::I32(((a as u32).wrapping_shr(b as u32)) as i32))
        }
        (N::I32Eq, C::I32(a), C::I32(b)) => compare_i32(a == b),
        (N::I32Ne, C::I32(a), C::I32(b)) => compare_i32(a != b),
        (N::I32LtS, C::I32(a), C::I32(b)) => compare_i32(a < b),
        (N::I32LeS, C::I32(a), C::I32(b)) => compare_i32(a <= b),
        (N::I32GtS, C::I32(a), C::I32(b)) => compare_i32(a > b),
        (N::I32GeS, C::I32(a), C::I32(b)) => compare_i32(a >= b),
        (N::BoolAnd, C::Boolean(a), C::Boolean(b)) => Some(C::Boolean(a && b)),
        (N::BoolOr, C::Boolean(a), C::Boolean(b)) => Some(C::Boolean(a || b)),
        (N::BoolEq, C::Boolean(a), C::Boolean(b)) => Some(C::Boolean(a == b)),
        (N::BoolNe, C::Boolean(a), C::Boolean(b)) => Some(C::Boolean(a != b)),
        (N::F64Add, C::F64(a), C::F64(b)) => {
            Some(C::F64((f64::from_bits(a) + f64::from_bits(b)).to_bits()))
        }
        (N::F64Sub, C::F64(a), C::F64(b)) => {
            Some(C::F64((f64::from_bits(a) - f64::from_bits(b)).to_bits()))
        }
        (N::F64Mul, C::F64(a), C::F64(b)) => {
            Some(C::F64((f64::from_bits(a) * f64::from_bits(b)).to_bits()))
        }
        (N::F64Div, C::F64(a), C::F64(b)) => {
            Some(C::F64((f64::from_bits(a) / f64::from_bits(b)).to_bits()))
        }
        (N::F64Eq, C::F64(a), C::F64(b)) => compare_f64(f64::from_bits(a) == f64::from_bits(b)),
        (N::F64Ne, C::F64(a), C::F64(b)) => compare_f64(f64::from_bits(a) != f64::from_bits(b)),
        (N::F64Lt, C::F64(a), C::F64(b)) => compare_f64(f64::from_bits(a) < f64::from_bits(b)),
        (N::F64Le, C::F64(a), C::F64(b)) => compare_f64(f64::from_bits(a) <= f64::from_bits(b)),
        (N::F64Gt, C::F64(a), C::F64(b)) => compare_f64(f64::from_bits(a) > f64::from_bits(b)),
        (N::F64Ge, C::F64(a), C::F64(b)) => compare_f64(f64::from_bits(a) >= f64::from_bits(b)),
        _ => None,
    }
}

fn fold_unary(op: UnaryOp, value: ScalarConstant) -> Option<ScalarConstant> {
    use ScalarConstant as C;
    use UnaryOp as U;
    match (op, value) {
        (U::I32Neg, C::I32(value)) => Some(C::I32(value.wrapping_neg())),
        (U::I32Complement, C::I32(value)) => Some(C::I32(!value)),
        (U::F64Neg, C::F64(bits)) => Some(C::F64((-f64::from_bits(bits)).to_bits())),
        (U::BoolNot, C::Boolean(value)) => Some(C::Boolean(!value)),
        (U::I32ToF64, C::I32(value)) => Some(C::F64((value as f64).to_bits())),
        (U::F64ToF32, C::F64(bits)) => Some(C::F32((f64::from_bits(bits) as f32).to_bits())),
        (U::F32ToF64, C::F32(bits)) => Some(C::F64((f32::from_bits(bits) as f64).to_bits())),
        (U::F64ToI32Sat, C::F64(bits)) => Some(C::I32(f64::from_bits(bits) as i32)),
        (U::BoolToI32, C::Boolean(value)) => Some(C::I32(i32::from(value))),
        (U::I32ToBool, C::I32(value)) => Some(C::Boolean(value != 0)),
        (U::I32Identity, C::I32(value)) => Some(C::I32(value)),
        _ => None,
    }
}
