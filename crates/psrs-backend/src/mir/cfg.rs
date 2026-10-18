#![allow(dead_code)]
//! Shared CFG edge analysis for MIR.
//!
//! MIR terminators carry no structuring hint: `Branch` has no `merge_block`,
//! and `Switch` has no join field. Every consumer that needs the join of a
//! branch or switch derives it from the CFG edges, so the derivation lives
//! here and is shared by the verifier, the optimizer, and the Wasm structurer.

use super::{BasicBlock, BlockId, Function, Terminator};
use std::collections::{HashMap, HashSet, VecDeque};

/// Every normal path after `start` must cross a required instruction before a
/// normal exit, checked publication, or another execution of `start`. Trapping
/// paths are excluded; this does not establish caught-host-trap cleanup.
pub(crate) fn normal_paths_cross(
    function: &Function,
    start: (BlockId, usize),
    required: impl Fn(&super::Instruction) -> bool,
    publication: impl Fn(&super::Instruction) -> bool,
) -> bool {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    if blocks
        .get(&start.0)
        .is_none_or(|block| start.1 >= block.instructions.len())
    {
        return false;
    }
    let mut pending = vec![(start.0, start.1 + 1)];
    let mut seen = HashSet::new();
    while let Some((id, index)) = pending.pop() {
        if !seen.insert((id, index)) {
            continue;
        }
        let Some(block) = blocks.get(&id) else {
            return false;
        };
        let mut crossed = false;
        for (offset, instruction) in block.instructions[index..].iter().enumerate() {
            if required(instruction)
                || matches!(instruction, super::Instruction::Unreachable { .. })
            {
                crossed = true;
                break;
            }
            if (id, index + offset) == start || publication(instruction) {
                return false;
            }
        }
        if crossed {
            continue;
        }
        match &block.terminator {
            Some(Terminator::Trap { .. }) => {}
            Some(
                Terminator::Jump { .. } | Terminator::Branch { .. } | Terminator::Switch { .. },
            ) => {
                pending.extend(
                    successors(block.terminator.as_ref().unwrap())
                        .into_iter()
                        .map(|id| (id, 0)),
                );
            }
            _ => return false,
        }
    }
    true
}

/// Can a later instruction be reached before the producer executes again?
/// `None` denotes malformed point/edge metadata, not an empty path set.
pub(crate) fn reachable_without(
    function: &Function,
    source: (BlockId, usize),
    target: (BlockId, usize),
    producer: (BlockId, usize),
) -> Option<bool> {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    for point in [source, target, producer] {
        if point.1 >= blocks.get(&point.0)?.instructions.len() {
            return None;
        }
    }
    let mut pending = vec![(source.0, source.1 + 1)];
    let mut seen = HashSet::new();
    while let Some((id, index)) = pending.pop() {
        if !seen.insert((id, index)) {
            continue;
        }
        let block = blocks.get(&id)?;
        let mut stopped = false;
        for (offset, instruction) in block.instructions[index..].iter().enumerate() {
            let point = (id, index + offset);
            if point == producer || matches!(instruction, super::Instruction::Unreachable { .. }) {
                stopped = true;
                break;
            }
            if point == target {
                return Some(true);
            }
        }
        if !stopped {
            pending.extend(
                successors(block.terminator.as_ref()?)
                    .into_iter()
                    .map(|id| (id, 0)),
            );
        }
    }
    Some(false)
}

/// The successor blocks named by a terminator's normal edges. Exception
/// unwind edges are not normal edges: a `Throw` has none (its handler is
/// reached by unwinding), and a handler block must never become a normal
/// successor.
pub(crate) fn successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Return { .. } | Terminator::Trap { .. } => Vec::new(),
        Terminator::Throw { .. } => Vec::new(),
        Terminator::Try { protected, .. } => vec![*protected],
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
        Terminator::ReturnCall { .. } | Terminator::ReturnCallRef { .. } => Vec::new(),
    }
}

/// The successor blocks named by a terminator's normal and unwind edges. A
/// `Try`'s clause handlers receive control when the armed region catches, and
/// a `Throw`'s named handler receives it from the throw point. This is the
/// edge set the exception design's dominance rule rechecks the try-region
/// partition on ("Dominance is rechecked on the CFG that includes unwind
/// edges"): it keeps a join shared by a protected path and a handler path out
/// of the protected region, which the normal-edge view alone would claim.
pub(crate) fn unwind_successors(terminator: &Terminator) -> Vec<BlockId> {
    let mut next = successors(terminator);
    match terminator {
        Terminator::Try {
            catches, catch_all, ..
        } => {
            next.extend(catches.iter().map(|(_, handler)| *handler));
            next.extend(catch_all);
        }
        Terminator::Throw {
            handler: Some(handler),
            ..
        } => next.push(*handler),
        _ => {}
    }
    next
}

/// The nearest common one-value descendant of every `target`, if one exists.
///
/// A candidate must be reachable from every target and have exactly one
/// parameter, so the structured `if`/`br_table` the structurer emits can
/// produce that value with ordinary Wasm result typing. The nearest candidate
/// (smallest worst-case distance, then smallest total distance) wins.
pub(crate) fn common_join(blocks: &[BasicBlock], targets: &[BlockId]) -> Option<BlockId> {
    let by_id = blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let distances = targets
        .iter()
        .map(|target| block_distances(&by_id, *target))
        .collect::<Vec<_>>();
    let first = distances.first()?;
    let mut candidates = first
        .keys()
        .filter(|candidate| {
            by_id
                .get(candidate)
                .is_some_and(|block| block.parameters.len() == 1)
                && distances
                    .iter()
                    .all(|reachable| reachable.contains_key(candidate))
        })
        .copied()
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| {
        let mut depths = distances
            .iter()
            .map(|reachable| reachable.get(candidate).copied().unwrap_or(u32::MAX))
            .collect::<Vec<_>>();
        depths.sort_unstable();
        (
            depths.last().copied().unwrap_or(u32::MAX),
            depths.iter().sum::<u32>(),
        )
    });
    candidates.into_iter().next()
}

/// Every block that is the derived join of some branch or switch.
pub(crate) fn join_blocks(function: &Function) -> HashSet<BlockId> {
    let mut joins = HashSet::new();
    for block in &function.blocks {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        let targets = match terminator {
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
            Terminator::Trap { .. }
            | Terminator::Return { .. }
            | Terminator::Jump { .. }
            | Terminator::Throw { .. }
            | Terminator::Try { .. }
            | Terminator::ReturnCall { .. }
            | Terminator::ReturnCallRef { .. } => continue,
        };
        if let Some(join) = common_join(&function.blocks, &targets) {
            joins.insert(join);
        }
    }
    joins
}

fn block_distances(by_id: &HashMap<BlockId, &BasicBlock>, entry: BlockId) -> HashMap<BlockId, u32> {
    let mut distances = HashMap::from([(entry, 0)]);
    let mut pending = VecDeque::from([entry]);
    while let Some(current) = pending.pop_front() {
        let Some(block) = by_id.get(&current) else {
            continue;
        };
        let Some(terminator) = &block.terminator else {
            continue;
        };
        let next_distance = distances[&current] + 1;
        for successor in successors(terminator) {
            if let std::collections::hash_map::Entry::Vacant(entry) = distances.entry(successor) {
                entry.insert(next_distance);
                pending.push_back(successor);
            }
        }
    }
    distances
}

/// Block dominators shared by SSA and lifecycle verification.
///
/// Dominance is only defined along paths from the entry. Blocks with no such
/// path are excluded from the result so that an unreachable predecessor cannot
/// shrink a reachable block's dominator set to nothing.
pub(crate) fn dominators(function: &Function) -> HashMap<BlockId, HashSet<BlockId>> {
    dominators_from(function, function.entry, None)
}

/// Reachability within a semantic region. `stop` belongs to the next region.
pub(crate) fn reachable_from(
    function: &Function,
    entry: BlockId,
    stop: Option<BlockId>,
) -> HashSet<BlockId> {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let mut reachable = HashSet::new();
    let mut pending = vec![entry];
    while let Some(id) = pending.pop() {
        if Some(id) == stop || !reachable.insert(id) {
            continue;
        }
        if let Some(terminator) = blocks.get(&id).and_then(|block| block.terminator.as_ref()) {
            pending.extend(successors(terminator));
        }
    }
    reachable
}

/// Dominance in the same CFG region used to resolve returned join values.
pub(crate) fn dominators_from(
    function: &Function,
    entry: BlockId,
    stop: Option<BlockId>,
) -> HashMap<BlockId, HashSet<BlockId>> {
    dominators_with(function, entry, stop, successors)
}

/// Dominance over the CFG that includes unwind edges — the try-region
/// partition's input. The shared joins of normal and exceptional paths keep
/// their full predecessor sets, so a join is never dominated by a protected
/// entry alone.
pub(crate) fn dominators_with_unwind_edges(
    function: &Function,
) -> HashMap<BlockId, HashSet<BlockId>> {
    dominators_with(function, function.entry, None, unwind_successors)
}

fn dominators_with(
    function: &Function,
    entry: BlockId,
    stop: Option<BlockId>,
    edge_successors: impl Fn(&Terminator) -> Vec<BlockId>,
) -> HashMap<BlockId, HashSet<BlockId>> {
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let mut reachable = HashSet::new();
    let mut pending = vec![entry];
    while let Some(id) = pending.pop() {
        if Some(id) == stop || !reachable.insert(id) {
            continue;
        }
        if let Some(terminator) = blocks.get(&id).and_then(|block| block.terminator.as_ref()) {
            pending.extend(edge_successors(terminator));
        }
    }
    let all = reachable.iter().copied().collect::<HashSet<_>>();
    let mut predecessors = reachable
        .iter()
        .copied()
        .map(|id| (id, HashSet::new()))
        .collect::<HashMap<BlockId, HashSet<BlockId>>>();
    for id in &reachable {
        let Some(terminator) = blocks.get(id).and_then(|block| block.terminator.as_ref()) else {
            continue;
        };
        for successor in edge_successors(terminator) {
            if let Some(preds) = predecessors.get_mut(&successor) {
                preds.insert(*id);
            }
        }
    }

    let mut dominators = reachable
        .iter()
        .copied()
        .map(|id| {
            let initial = if id == entry {
                HashSet::from([entry])
            } else {
                all.clone()
            };
            (id, initial)
        })
        .collect::<HashMap<_, _>>();
    loop {
        let mut changed = false;
        for id in reachable.iter().copied().filter(|id| *id != entry) {
            let mut next = all.clone();
            if let Some(preds) = predecessors.get(&id) {
                if preds.is_empty() {
                    next.clear();
                } else {
                    for pred in preds {
                        if let Some(pred_dominators) = dominators.get(pred) {
                            next.retain(|candidate| pred_dominators.contains(candidate));
                        }
                    }
                }
            }
            next.insert(id);
            if dominators.get(&id) != Some(&next) {
                dominators.insert(id, next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    dominators
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::Terminator;
    use crate::types::ValueId;
    use psrs_span::TextRange;

    fn span() -> TextRange {
        TextRange::new(0, 1)
    }

    fn block(id: u32, parameters: Vec<u32>, terminator: Terminator) -> BasicBlock {
        BasicBlock {
            id: BlockId(id),
            parameters: parameters.into_iter().map(ValueId).collect(),
            instructions: Vec::new(),
            terminator: Some(terminator),
        }
    }

    fn branch(then_block: u32, else_block: u32) -> Terminator {
        Terminator::Branch {
            condition: ValueId(0),
            then_block: BlockId(then_block),
            else_block: BlockId(else_block),
            span: span(),
        }
    }

    fn jump(target: u32) -> Terminator {
        Terminator::Jump {
            target: BlockId(target),
            arguments: Vec::new(),
            span: span(),
        }
    }

    fn return_empty() -> Terminator {
        Terminator::Return {
            value: ValueId(0),
            span: span(),
        }
    }

    #[test]
    fn derives_a_diamond_join_from_the_edges() {
        let blocks = vec![
            block(0, vec![], branch(1, 2)),
            block(1, vec![], jump(3)),
            block(2, vec![], jump(3)),
            block(3, vec![7], return_empty()),
        ];
        assert_eq!(
            common_join(&blocks, &[BlockId(1), BlockId(2)]),
            Some(BlockId(3))
        );
    }

    #[test]
    fn derives_the_nearest_common_join() {
        // B0 -> {B1, B2}; B1 -> {B3, B4} -> B5; B2 -> B6; B5 -> B6.
        // The inner join B5 is not reachable from B2, so the outer join is B6.
        let blocks = vec![
            block(0, vec![], branch(1, 2)),
            block(1, vec![], branch(3, 4)),
            block(2, vec![], jump(6)),
            block(3, vec![], jump(5)),
            block(4, vec![], jump(5)),
            block(5, vec![8], jump(6)),
            block(6, vec![9], return_empty()),
        ];
        assert_eq!(
            common_join(&blocks, &[BlockId(3), BlockId(4)]),
            Some(BlockId(5))
        );
        assert_eq!(
            common_join(&blocks, &[BlockId(1), BlockId(2)]),
            Some(BlockId(6))
        );
    }

    #[test]
    fn reports_no_join_when_arms_terminate_independently() {
        let blocks = vec![
            block(0, vec![], branch(1, 2)),
            block(1, vec![], return_empty()),
            block(2, vec![], return_empty()),
        ];
        assert_eq!(common_join(&blocks, &[BlockId(1), BlockId(2)]), None);
    }
}
