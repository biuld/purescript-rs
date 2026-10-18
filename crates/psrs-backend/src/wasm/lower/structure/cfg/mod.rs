use super::wasm_error;
use crate::BackendError;
use crate::mir::try_region;
use crate::mir::{BasicBlock, BlockId, Function, Tag, Terminator};
use crate::types::ValueType;
use std::collections::{HashMap, HashSet};

mod build;
mod irreducible;
mod try_context;

use build::build_region;
use irreducible::has_irreducible_component;
use try_context::TryContext;

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
    /// EH-10: a composite unit covering the try block, the protected
    /// sub-region, and the handler, occupying one slot in the enclosing
    /// region's topological order. Handler blocks are not units of their
    /// own; the composite unit emits each handler inline after the unwind
    /// block.
    Try {
        try_block: BlockId,
        protected: Box<RegionPlan>,
        /// The one handler every clause of the region delivers to; `None`
        /// when the region arms no clauses at all.
        handler: Option<BlockId>,
        /// The caught payload delivered through the synthesized unwind
        /// block; `None` when the clauses are payload-less (`catch_all`).
        payload: Option<ValueType>,
    },
}

impl UnitPlan {
    pub(super) fn entry(&self) -> BlockId {
        match self {
            Self::Block(block)
            | Self::Loop { header: block, .. }
            | Self::Try {
                try_block: block, ..
            } => *block,
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
        tags: &[Tag],
    ) -> Result<Self, Vec<BackendError>> {
        // One definition of the protected-region partition, shared with the
        // verifier and tail-call marking. Fails for malformed unwind edges;
        // those are already diagnosed at P9.
        let try_regions = try_region::plan(function, "P10 Wasm structuring")?;
        let reachable = reachable_blocks(function.entry, blocks, function.span)?;
        let predecessors = predecessors(&reachable, blocks);
        if has_irreducible_component(function.entry, &reachable, &predecessors, blocks) {
            // EH-11: the dispatcher's state machine cannot arm a try_table.
            if let Some(region) = try_regions.regions.first() {
                return Err(wasm_error(
                    region.span,
                    "MIR function combines irreducible control flow with a try region; the dispatcher cannot arm a try_table",
                ));
            }
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
        let try_context = TryContext::new(try_regions, tags, &loops)?;
        let root = build_region(
            function,
            blocks,
            &reachable_set,
            &loops,
            None,
            function.entry,
            &try_context,
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
        // The region plan must cover the catch paths too: a handler and its
        // successors execute when the armed region catches, so the walk
        // follows the unwind edges as well as the normal ones. Try-free
        // terminators have no unwind edges, so this is the same walk as
        // before for them.
        let mut next = successors(terminator);
        next.extend(unwind_only_successors(terminator));
        pending.extend(next.into_iter().rev());
    }
    reachable.sort_by_key(|block| block.0);
    Ok(reachable)
}

/// The unwind edges alone, so the region plan's walk can follow the catch
/// paths without re-listing the normal successors.
fn unwind_only_successors(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Try {
            catches, catch_all, ..
        } => catches
            .iter()
            .map(|(_, handler)| *handler)
            .chain(catch_all.iter().copied())
            .collect(),
        Terminator::Throw {
            handler: Some(handler),
            ..
        } => vec![*handler],
        _ => Vec::new(),
    }
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

fn successors(terminator: &Terminator) -> Vec<BlockId> {
    crate::mir::cfg::successors(terminator)
}

fn block_span(block: &BasicBlock, fallback: psrs_span::TextRange) -> psrs_span::TextRange {
    block
        .terminator
        .as_ref()
        .map_or(fallback, |terminator| match terminator {
            Terminator::Trap { span }
            | Terminator::Return { span, .. }
            | Terminator::Jump { span, .. }
            | Terminator::Branch { span, .. }
            | Terminator::Switch { span, .. }
            | Terminator::ReturnCall { span, .. }
            | Terminator::ReturnCallRef { span, .. }
            | Terminator::Throw { span, .. }
            | Terminator::Try { span, .. } => *span,
        })
}

pub(super) fn unit_span(
    unit: &UnitPlan,
    blocks: &HashMap<BlockId, &BasicBlock>,
    fallback: psrs_span::TextRange,
) -> psrs_span::TextRange {
    match unit {
        UnitPlan::Block(block)
        | UnitPlan::Loop { header: block, .. }
        | UnitPlan::Try {
            try_block: block, ..
        } => blocks
            .get(block)
            .map_or(fallback, |block| block_span(block, fallback)),
    }
}
