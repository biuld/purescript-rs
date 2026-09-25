//! Shared CFG edge analysis for MIR.
//!
//! MIR terminators carry no structuring hint: `Branch` has no `merge_block`,
//! and `Switch` has no join field. Every consumer that needs the join of a
//! branch or switch derives it from the CFG edges, so the derivation lives
//! here and is shared by the verifier, the optimizer, and the Wasm structurer.

use super::{BasicBlock, BlockId, Function, Terminator};
use std::collections::{HashMap, HashSet, VecDeque};

/// The successor blocks named by a terminator.
pub(crate) fn successors(terminator: &Terminator) -> Vec<BlockId> {
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
        Terminator::ReturnCall { .. } | Terminator::ReturnCallRef { .. } => Vec::new(),
    }
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
            Terminator::Return { .. }
            | Terminator::Jump { .. }
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
