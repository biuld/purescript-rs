//! CFG reachability and terminator simplification.

use super::constants;
use crate::mir::{BasicBlock, BlockId, Module, Terminator};
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) fn prune_unreachable(module: &mut Module) -> bool {
    let mut changed = false;
    for function in &mut module.functions {
        let reachable = reachable_blocks(function.entry, &function.blocks);
        // Exception handlers are reached only through unwind edges, which the
        // normal-successor walk does not follow; keep every handler and
        // everything it reaches normally.
        let reachable = match crate::mir::try_region::handler_closure(function) {
            handlers if handlers.is_empty() => reachable,
            handlers => {
                let by_id = function
                    .blocks
                    .iter()
                    .map(|block| (block.id, block))
                    .collect::<HashMap<_, _>>();
                let mut kept = reachable;
                let mut pending = VecDeque::from_iter(handlers);
                while let Some(id) = pending.pop_back() {
                    if !kept.insert(id) {
                        continue;
                    }
                    if let Some(terminator) =
                        by_id.get(&id).and_then(|block| block.terminator.as_ref())
                    {
                        pending.extend(crate::mir::cfg::successors(terminator));
                    }
                }
                kept
            }
        };
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
                let terminator = block.terminator.as_ref()?;
                let (target, span) = match terminator {
                    Terminator::Branch {
                        condition,
                        then_block,
                        else_block,
                        span,
                        ..
                    } => {
                        let target = if then_block == else_block {
                            Some(*then_block)
                        } else {
                            constants::boolean_constant(function, *condition)
                                .map(|condition| if condition { *then_block } else { *else_block })
                        }?;
                        (target, *span)
                    }
                    Terminator::Switch {
                        value,
                        cases,
                        default,
                        span,
                    } => {
                        let value = constants::integer_constant(function, *value)?;
                        let target = cases
                            .iter()
                            .find_map(|(case, target)| (*case == value).then_some(*target))
                            .unwrap_or(*default);
                        (target, *span)
                    }
                    Terminator::Trap { .. }
                    | Terminator::Return { .. }
                    | Terminator::Jump { .. }
                    | Terminator::Throw { .. }
                    | Terminator::Try { .. }
                    | Terminator::ReturnCall { .. }
                    | Terminator::ReturnCallRef { .. } => return None,
                };
                Some((block.id, (target, span)))
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
    crate::mir::cfg::successors(terminator)
}
