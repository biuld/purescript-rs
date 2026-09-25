use super::util::mir_error;
use crate::BackendError;
use crate::mir::{BlockId, Function, Instruction, Terminator};
use crate::types::ValueId;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

pub(super) fn verify_array_maps(function: &Function) -> Result<(), Vec<BackendError>> {
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Instruction::ArrayNewDefault {
                destination,
                source,
                length,
                header,
                body,
                exit,
                index,
                span,
                ..
            } = instruction
            {
                verify_array_map(
                    function,
                    block.id,
                    *destination,
                    *source,
                    *length,
                    *header,
                    *body,
                    *exit,
                    *index,
                    *span,
                )?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn verify_array_map(
    function: &Function,
    allocation_block: BlockId,
    array: ValueId,
    source: ValueId,
    length: ValueId,
    header_id: BlockId,
    body_id: BlockId,
    exit_id: BlockId,
    index: ValueId,
    span: psrs_span::TextRange,
) -> Result<(), Vec<BackendError>> {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let Some(allocation) = blocks.get(&allocation_block) else {
        return Err(mir_error(
            span,
            "array conversion allocation block is missing",
        ));
    };
    let Some(header) = blocks.get(&header_id) else {
        return Err(mir_error(span, "array conversion loop header is missing"));
    };
    let Some(body) = blocks.get(&body_id) else {
        return Err(mir_error(span, "array conversion loop body is missing"));
    };
    let Some(exit) = blocks.get(&exit_id) else {
        return Err(mir_error(span, "array conversion loop exit is missing"));
    };
    if header.parameters != [index] || !body.parameters.is_empty() || !exit.parameters.is_empty() {
        return Err(mir_error(
            span,
            "array conversion loop has invalid block parameters",
        ));
    }
    if !allocation.instructions.iter().any(|instruction| {
        matches!(instruction, Instruction::ArrayLen { destination, value, .. }
            if *destination == length && *value == source)
    }) {
        return Err(mir_error(
            span,
            "array conversion length does not come from its source",
        ));
    }
    let Some(Terminator::Jump {
        target, arguments, ..
    }) = &allocation.terminator
    else {
        return Err(mir_error(
            span,
            "array conversion allocation must enter its loop",
        ));
    };
    let is_zero = arguments.first().is_some_and(|initial| {
        *target == header_id
            && arguments.len() == 1
            && allocation.instructions.iter().any(|instruction| {
                matches!(instruction, Instruction::Constant { destination, value: 0, .. }
                    if destination == initial)
            })
    });
    if !is_zero {
        return Err(mir_error(
            span,
            "array conversion loop must start at index zero",
        ));
    }
    let Some(Terminator::Branch {
        condition,
        then_block,
        else_block,
        ..
    }) = &header.terminator
    else {
        return Err(mir_error(
            span,
            "array conversion loop header must branch on its bound",
        ));
    };
    if *then_block != body_id
        || *else_block != exit_id
        || !header.instructions.iter().any(|instruction| {
            matches!(instruction, Instruction::Primitive {
                destination,
                op: crate::mir::NumericOp::I32LtS,
                left,
                right,
                ..
            } if destination == condition && *left == index && *right == length)
        })
    {
        return Err(mir_error(
            span,
            "array conversion loop bound is not index < length",
        ));
    }

    let stores = function
        .blocks
        .iter()
        .filter(|block| {
            block.instructions.iter().any(|instruction| {
                matches!(instruction, Instruction::ArraySet { value, index: store_index, .. }
                    if *value == array && *store_index == index)
            })
        })
        .map(|block| block.id)
        .collect::<HashSet<_>>();
    if stores.is_empty() {
        return Err(mir_error(
            span,
            "array conversion loop never initializes its destination",
        ));
    }
    let loop_blocks = reachable_loop_blocks(body_id, header_id, exit_id, &blocks, span)?;
    if stores.iter().any(|store| !loop_blocks.contains(store)) {
        return Err(mir_error(
            span,
            "array conversion initialization is outside its loop",
        ));
    }
    if path_escapes_without_store(body_id, header_id, exit_id, array, index, &blocks) {
        return Err(mir_error(
            span,
            "array conversion can leave an iteration without initializing its element",
        ));
    }
    for block_id in loop_blocks
        .iter()
        .copied()
        .chain([allocation_block, header_id])
    {
        let Some(block) = blocks.get(&block_id) else {
            continue;
        };
        for instruction in &block.instructions {
            if instruction.operands().contains(&array)
                && !matches!(instruction, Instruction::ArraySet { value, index: store_index, .. }
                    if *value == array && *store_index == index && !matches!(instruction, Instruction::ArraySet { new_value, .. } if *new_value == array))
            {
                return Err(mir_error(
                    span,
                    "array conversion destination escapes before initialization completes",
                ));
            }
        }
        if matches!(&block.terminator, Some(Terminator::Return { value, .. }) if *value == array)
            || matches!(&block.terminator, Some(Terminator::Jump { arguments, .. }) if arguments.contains(&array))
            || matches!(&block.terminator, Some(Terminator::Branch { condition, .. }) if *condition == array)
            || matches!(&block.terminator, Some(Terminator::Switch { value, .. }) if *value == array)
        {
            return Err(mir_error(
                span,
                "array conversion destination escapes before initialization completes",
            ));
        }
    }
    verify_loop_increment(function, allocation_block, header_id, index, &blocks, span)
}

fn reachable_loop_blocks(
    start: BlockId,
    header: BlockId,
    exit: BlockId,
    blocks: &HashMap<BlockId, &crate::mir::BasicBlock>,
    span: psrs_span::TextRange,
) -> Result<HashSet<BlockId>, Vec<BackendError>> {
    let mut seen = HashSet::new();
    let mut pending = vec![start];
    while let Some(id) = pending.pop() {
        if id == header || id == exit || !seen.insert(id) {
            continue;
        }
        let Some(block) = blocks.get(&id) else {
            return Err(mir_error(
                span,
                "array conversion loop branches to a missing block",
            ));
        };
        if let Some(terminator) = &block.terminator {
            pending.extend(successors(terminator));
        }
    }
    Ok(seen)
}

fn path_escapes_without_store(
    start: BlockId,
    header: BlockId,
    exit: BlockId,
    array: ValueId,
    index: ValueId,
    blocks: &HashMap<BlockId, &crate::mir::BasicBlock>,
) -> bool {
    let mut seen = HashSet::new();
    let mut pending = vec![(start, false)];
    while let Some((id, initialized)) = pending.pop() {
        if id == exit || (id == header && !initialized) {
            return true;
        }
        if id == header || !seen.insert((id, initialized)) {
            continue;
        }
        let Some(block) = blocks.get(&id) else {
            return true;
        };
        let stored = initialized
            || block.instructions.iter().any(|instruction| {
                matches!(instruction, Instruction::ArraySet { value, index: store_index, .. }
                if *value == array && *store_index == index)
            });
        if let Some(terminator) = &block.terminator {
            if matches!(terminator, Terminator::Return { .. }) {
                return true;
            }
            pending.extend(
                successors(terminator)
                    .into_iter()
                    .map(|next| (next, stored)),
            );
        } else {
            return true;
        }
    }
    false
}

fn verify_loop_increment(
    function: &Function,
    allocation_block: BlockId,
    header: BlockId,
    index: ValueId,
    blocks: &HashMap<BlockId, &crate::mir::BasicBlock>,
    span: psrs_span::TextRange,
) -> Result<(), Vec<BackendError>> {
    let constants = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction {
            Instruction::Constant {
                destination,
                value: 1,
                ..
            } => Some(*destination),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let backedges = blocks
        .values()
        .filter_map(|block| match &block.terminator {
            Some(Terminator::Jump {
                target, arguments, ..
            }) if *target == header && block.id != allocation_block => Some((*block, arguments)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if backedges.is_empty()
        || backedges.iter().any(|(block, arguments)| {
            arguments.len() != 1
                || !block.instructions.iter().any(|instruction| {
                    matches!(instruction, Instruction::Primitive {
                        destination,
                        op: crate::mir::NumericOp::I32Add,
                        left,
                        right,
                        ..
                    } if Some(destination) == arguments.first()
                        && ((*left == index && constants.contains(right))
                            || (*right == index && constants.contains(left))))
                })
        })
    {
        return Err(mir_error(
            span,
            "array conversion loop must advance its index by one",
        ));
    }
    Ok(())
}

fn successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Return { .. } => Vec::new(),
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        Terminator::Switch { cases, default, .. } => cases
            .iter()
            .map(|(_, target)| *target)
            .chain(std::iter::once(*default))
            .collect(),
    }
}
