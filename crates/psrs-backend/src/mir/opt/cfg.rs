//! CFG reachability and terminator simplification.

use super::constants;
use crate::mir::{BasicBlock, BlockId, Module, Terminator};
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) fn prune_unreachable(module: &mut Module) -> bool {
    let mut changed = false;
    for function in &mut module.functions {
        let reachable = reachable_blocks(function.entry, &function.blocks);
        let original_len = function.blocks.len();
        function
            .blocks
            .retain(|block| reachable.contains(&block.id));
        changed |= function.blocks.len() != original_len;
    }
    changed
}

pub(super) fn simplify_terminators(module: &mut Module) -> bool {
    let mut changed = false;
    for function in &mut module.functions {
        let rewrites = function
            .blocks
            .iter()
            .filter_map(|block| {
                let Some(Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                    span,
                    ..
                }) = block.terminator.as_ref()
                else {
                    return None;
                };
                let target = if then_block == else_block {
                    Some(*then_block)
                } else {
                    constants::boolean_constant(function, *condition)
                        .map(|condition| if condition { *then_block } else { *else_block })
                }?;
                Some((block.id, (target, *span)))
            })
            .collect::<HashMap<_, _>>();
        for block in &mut function.blocks {
            if let Some((target, span)) = rewrites.get(&block.id) {
                block.terminator = Some(Terminator::Jump {
                    target: *target,
                    arguments: Vec::new(),
                    span: *span,
                });
                changed = true;
            }
        }
    }
    changed
}

pub(super) fn reachable_blocks(entry: BlockId, blocks: &[BasicBlock]) -> HashSet<BlockId> {
    let by_id = blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let mut reachable = HashSet::new();
    let mut queue = VecDeque::from([entry]);
    while let Some(id) = queue.pop_front() {
        if !reachable.insert(id) {
            continue;
        }
        let Some(block) = by_id.get(&id) else {
            continue;
        };
        if let Some(terminator) = &block.terminator {
            for successor in successors(terminator) {
                queue.push_back(successor);
            }
        }
    }
    reachable
}

pub(super) fn successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Return { .. } => Vec::new(),
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
    }
}
