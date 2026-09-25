use super::wasm_error;
use crate::BackendError;
use crate::mir::{BasicBlock, BlockId, Function, Terminator};
use std::collections::{HashMap, HashSet};

mod irreducible;

use irreducible::has_irreducible_component;

#[derive(Clone, Debug)]
pub(super) enum ControlFlowPlan {
    Reducible(RegionPlan),
    Dispatcher(Vec<BlockId>),
}

#[derive(Clone, Debug)]
pub(super) struct RegionPlan {
    pub(super) header: Option<BlockId>,
    pub(super) units: Vec<UnitPlan>,
    pub(super) span: psrs_span::TextRange,
}

#[derive(Clone, Debug)]
pub(super) enum UnitPlan {
    Block(BlockId),
    Loop {
        header: BlockId,
        body: Box<RegionPlan>,
    },
}

impl UnitPlan {
    pub(super) fn entry(&self) -> BlockId {
        match self {
            Self::Block(block) | Self::Loop { header: block, .. } => *block,
        }
    }
}

#[derive(Clone, Debug)]
struct NaturalLoop {
    header: BlockId,
    blocks: HashSet<BlockId>,
}

impl ControlFlowPlan {
    pub(super) fn build(
        function: &Function,
        blocks: &HashMap<BlockId, &BasicBlock>,
    ) -> Result<Self, Vec<BackendError>> {
        let reachable = reachable_blocks(function.entry, blocks, function.span)?;
        let predecessors = predecessors(&reachable, blocks);
        if has_irreducible_component(function.entry, &reachable, &predecessors, blocks) {
            return Ok(Self::Dispatcher(reachable));
        }
        let dominators = dominators(function.entry, &reachable, &predecessors);
        let loops = natural_loops(&reachable, &predecessors, &dominators, blocks);
        validate_loop_nesting(&loops, function.span)?;
        let reachable_set = reachable.iter().copied().collect::<HashSet<_>>();
        for loop_ in &loops {
            if loop_.blocks.iter().any(|block| {
                !dominators
                    .get(block)
                    .is_some_and(|block_dominators| block_dominators.contains(&loop_.header))
            }) {
                return Err(wasm_error(
                    function.span,
                    "MIR natural loop has an entry not dominated by its header; the control flow is irreducible",
                ));
            }
        }
        let root = build_region(
            function,
            blocks,
            &reachable_set,
            &loops,
            None,
            function.entry,
        )?;
        Ok(Self::Reducible(root))
    }
}

fn reachable_blocks(
    entry: BlockId,
    blocks: &HashMap<BlockId, &BasicBlock>,
    span: psrs_span::TextRange,
) -> Result<Vec<BlockId>, Vec<BackendError>> {
    if !blocks.contains_key(&entry) {
        return Err(wasm_error(span, "MIR entry block does not exist"));
    }
    let mut reachable = Vec::new();
    let mut seen = HashSet::new();
    let mut pending = vec![entry];
    while let Some(block_id) = pending.pop() {
        if !seen.insert(block_id) {
            continue;
        }
        let Some(block) = blocks.get(&block_id) else {
            return Err(wasm_error(span, "MIR branch target is missing"));
        };
        let Some(terminator) = &block.terminator else {
            return Err(wasm_error(span, "MIR block has no terminator"));
        };
        reachable.push(block_id);
        pending.extend(successors(terminator).into_iter().rev());
    }
    reachable.sort_by_key(|block| block.0);
    Ok(reachable)
}

fn predecessors(
    reachable: &[BlockId],
    blocks: &HashMap<BlockId, &BasicBlock>,
) -> HashMap<BlockId, HashSet<BlockId>> {
    let reachable_set = reachable.iter().copied().collect::<HashSet<_>>();
    let mut result = reachable
        .iter()
        .copied()
        .map(|block| (block, HashSet::new()))
        .collect::<HashMap<_, _>>();
    for source in reachable {
        let Some(terminator) = blocks
            .get(source)
            .and_then(|block| block.terminator.as_ref())
        else {
            continue;
        };
        for target in successors(terminator) {
            if reachable_set.contains(&target) {
                result.entry(target).or_default().insert(*source);
            }
        }
    }
    result
}

fn dominators(
    entry: BlockId,
    reachable: &[BlockId],
    predecessors: &HashMap<BlockId, HashSet<BlockId>>,
) -> HashMap<BlockId, HashSet<BlockId>> {
    let all = reachable.iter().copied().collect::<HashSet<_>>();
    let mut result = reachable
        .iter()
        .copied()
        .map(|block| {
            let initial = if block == entry {
                HashSet::from([entry])
            } else {
                all.clone()
            };
            (block, initial)
        })
        .collect::<HashMap<_, _>>();
    loop {
        let mut changed = false;
        for block in reachable.iter().copied().filter(|block| *block != entry) {
            let preds = predecessors.get(&block).cloned().unwrap_or_default();
            let mut next = if let Some(first) = preds.iter().next() {
                result.get(first).cloned().unwrap_or_default()
            } else {
                HashSet::new()
            };
            for predecessor in preds.iter().skip(1) {
                if let Some(dom) = result.get(predecessor) {
                    next.retain(|candidate| dom.contains(candidate));
                }
            }
            next.insert(block);
            if result.get(&block) != Some(&next) {
                result.insert(block, next);
                changed = true;
            }
        }
        if !changed {
            return result;
        }
    }
}

fn natural_loops(
    reachable: &[BlockId],
    predecessors: &HashMap<BlockId, HashSet<BlockId>>,
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
    blocks: &HashMap<BlockId, &BasicBlock>,
) -> Vec<NaturalLoop> {
    let mut by_header = HashMap::<BlockId, HashSet<BlockId>>::new();
    for source in reachable {
        let Some(terminator) = blocks
            .get(source)
            .and_then(|block| block.terminator.as_ref())
        else {
            continue;
        };
        for header in successors(terminator) {
            if !dominators
                .get(source)
                .is_some_and(|source_dominators| source_dominators.contains(&header))
            {
                continue;
            }
            let loop_blocks = by_header.entry(header).or_default();
            loop_blocks.insert(header);
            loop_blocks.insert(*source);
            let mut pending = if *source == header {
                Vec::new()
            } else {
                vec![*source]
            };
            while let Some(block) = pending.pop() {
                for predecessor in predecessors.get(&block).into_iter().flatten() {
                    if loop_blocks.insert(*predecessor) && *predecessor != header {
                        pending.push(*predecessor);
                    }
                }
            }
        }
    }
    let mut result = by_header
        .into_iter()
        .map(|(header, blocks)| NaturalLoop { header, blocks })
        .collect::<Vec<_>>();
    result.sort_by_key(|loop_| (loop_.blocks.len(), loop_.header.0));
    result
}

fn validate_loop_nesting(
    loops: &[NaturalLoop],
    span: psrs_span::TextRange,
) -> Result<(), Vec<BackendError>> {
    for (index, left) in loops.iter().enumerate() {
        for right in &loops[index + 1..] {
            let overlaps = left.blocks.iter().any(|block| right.blocks.contains(block));
            if overlaps
                && !left.blocks.is_subset(&right.blocks)
                && !right.blocks.is_subset(&left.blocks)
            {
                return Err(wasm_error(
                    span,
                    "MIR natural loops overlap without nesting; the control flow is irreducible",
                ));
            }
            if left.blocks == right.blocks && left.header != right.header {
                return Err(wasm_error(
                    span,
                    "MIR natural loops have conflicting headers",
                ));
            }
        }
    }
    Ok(())
}

fn build_region(
    function: &Function,
    blocks: &HashMap<BlockId, &BasicBlock>,
    members: &HashSet<BlockId>,
    loops: &[NaturalLoop],
    header: Option<BlockId>,
    entry: BlockId,
) -> Result<RegionPlan, Vec<BackendError>> {
    let children = direct_children(members, loops, header);
    let mut owner = HashMap::<BlockId, usize>::new();
    for (index, loop_) in children.iter().enumerate() {
        for block in &loop_.blocks {
            if owner.insert(*block, index).is_some() {
                return Err(wasm_error(
                    function.span,
                    "MIR natural loop regions overlap without a unique nesting order",
                ));
            }
        }
    }

    let mut units = Vec::<UnitPlan>::new();
    let mut unit_blocks = Vec::<HashSet<BlockId>>::new();
    let mut unit_for_block = HashMap::<BlockId, usize>::new();
    for loop_ in &children {
        let index = units.len();
        units.push(UnitPlan::Loop {
            header: loop_.header,
            body: Box::new(build_region(
                function,
                blocks,
                &loop_.blocks,
                loops,
                Some(loop_.header),
                loop_.header,
            )?),
        });
        unit_blocks.push(loop_.blocks.clone());
        for block in &loop_.blocks {
            unit_for_block.insert(*block, index);
        }
    }
    for block in members {
        if owner.contains_key(block) {
            continue;
        }
        let index = units.len();
        units.push(UnitPlan::Block(*block));
        unit_blocks.push(HashSet::from([*block]));
        unit_for_block.insert(*block, index);
    }

    let mut edges = vec![HashSet::<usize>::new(); units.len()];
    let mut indegree = vec![0usize; units.len()];
    for (source_unit, source_blocks) in unit_blocks.iter().enumerate() {
        for source in source_blocks {
            let Some(terminator) = blocks
                .get(source)
                .and_then(|block| block.terminator.as_ref())
            else {
                return Err(wasm_error(function.span, "MIR block has no terminator"));
            };
            for target in successors(terminator) {
                if Some(target) == header || !members.contains(&target) {
                    continue;
                }
                let Some(target_unit) = unit_for_block.get(&target).copied() else {
                    return Err(wasm_error(function.span, "MIR branch target has no region"));
                };
                if source_unit != target_unit && edges[source_unit].insert(target_unit) {
                    indegree[target_unit] += 1;
                }
            }
        }
    }

    let entry_unit = unit_for_block.get(&entry).copied().ok_or_else(|| {
        wasm_error(
            function.span,
            "MIR region entry is not part of its control-flow region",
        )
    })?;
    let mut order = Vec::with_capacity(units.len());
    let mut emitted = vec![false; units.len()];
    while order.len() < units.len() {
        let next = (0..units.len())
            .filter(|index| !emitted[*index] && indegree[*index] == 0)
            .min_by_key(|index| (usize::from(*index != entry_unit), units[*index].entry().0));
        let Some(next) = next else {
            return Err(wasm_error(
                function.span,
                "MIR contains a cycle without a natural-loop header; the control flow is irreducible",
            ));
        };
        emitted[next] = true;
        order.push(next);
        for target in &edges[next] {
            indegree[*target] -= 1;
        }
    }
    let sorted_units = order
        .into_iter()
        .map(|index| units[index].clone())
        .collect();
    let span = header
        .and_then(|header| blocks.get(&header))
        .map_or(function.span, |block| block_span(block, function.span));
    Ok(RegionPlan {
        header,
        units: sorted_units,
        span,
    })
}

fn direct_children<'a>(
    members: &HashSet<BlockId>,
    loops: &'a [NaturalLoop],
    header: Option<BlockId>,
) -> Vec<&'a NaturalLoop> {
    let candidates = loops
        .iter()
        .filter(|loop_| {
            Some(loop_.header) != header
                && loop_.blocks.is_subset(members)
                && (header.is_none() || loop_.blocks.len() < members.len())
        })
        .collect::<Vec<_>>();
    candidates
        .iter()
        .copied()
        .filter(|candidate| {
            !candidates.iter().any(|middle| {
                candidate.header != middle.header
                    && candidate.blocks.is_subset(&middle.blocks)
                    && middle.blocks.is_subset(members)
                    && candidate.blocks.len() < middle.blocks.len()
                    && middle.blocks.len() < members.len()
            })
        })
        .collect()
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

fn block_span(block: &BasicBlock, fallback: psrs_span::TextRange) -> psrs_span::TextRange {
    block
        .terminator
        .as_ref()
        .map_or(fallback, |terminator| match terminator {
            Terminator::Return { span, .. }
            | Terminator::Jump { span, .. }
            | Terminator::Branch { span, .. }
            | Terminator::Switch { span, .. } => *span,
        })
}

pub(super) fn unit_span(
    unit: &UnitPlan,
    blocks: &HashMap<BlockId, &BasicBlock>,
    fallback: psrs_span::TextRange,
) -> psrs_span::TextRange {
    match unit {
        UnitPlan::Block(block) | UnitPlan::Loop { header: block, .. } => blocks
            .get(block)
            .map_or(fallback, |block| block_span(block, fallback)),
    }
}
