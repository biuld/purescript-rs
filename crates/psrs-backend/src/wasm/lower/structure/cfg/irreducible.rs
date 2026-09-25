use crate::mir::{BasicBlock, BlockId};
use std::collections::{HashMap, HashSet};

use super::successors;

pub(super) fn has_irreducible_component(
    entry: BlockId,
    reachable: &[BlockId],
    predecessors: &HashMap<BlockId, HashSet<BlockId>>,
    blocks: &HashMap<BlockId, &BasicBlock>,
) -> bool {
    let reachable_set = reachable.iter().copied().collect::<HashSet<_>>();
    let successors = reachable
        .iter()
        .copied()
        .map(|block_id| {
            let next = blocks
                .get(&block_id)
                .and_then(|block| block.terminator.as_ref())
                .map(successors)
                .unwrap_or_default()
                .into_iter()
                .filter(|target| reachable_set.contains(target))
                .collect::<Vec<_>>();
            (block_id, next)
        })
        .collect::<HashMap<_, _>>();

    let mut finished = Vec::with_capacity(reachable.len());
    let mut visited = HashSet::new();
    for root in reachable {
        if !visited.insert(*root) {
            continue;
        }
        let mut stack = vec![(*root, 0usize)];
        while !stack.is_empty() {
            let (current, next_child) = {
                let (current, next_child) = stack.last_mut().expect("stack is not empty");
                (*current, next_child)
            };
            let children = successors.get(&current).map_or(&[][..], Vec::as_slice);
            if *next_child < children.len() {
                let child = children[*next_child];
                *next_child += 1;
                if visited.insert(child) {
                    stack.push((child, 0));
                }
            } else {
                finished.push(stack.pop().expect("stack is not empty").0);
            }
        }
    }

    let mut assigned = HashSet::new();
    for root in finished.into_iter().rev() {
        if !assigned.insert(root) {
            continue;
        }
        let mut component = Vec::new();
        let mut pending = vec![root];
        while let Some(current) = pending.pop() {
            component.push(current);
            for predecessor in predecessors.get(&current).into_iter().flatten() {
                if assigned.insert(*predecessor) {
                    pending.push(*predecessor);
                }
            }
        }

        let cyclic = component.len() > 1
            || successors
                .get(&root)
                .is_some_and(|next| next.contains(&root));
        if !cyclic {
            continue;
        }

        let members = component.iter().copied().collect::<HashSet<_>>();
        let entries = component
            .iter()
            .filter(|block| {
                **block == entry
                    || predecessors
                        .get(block)
                        .is_some_and(|incoming| incoming.iter().any(|from| !members.contains(from)))
            })
            .count();
        if entries > 1 {
            return true;
        }
    }
    false
}
