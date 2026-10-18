//! Construction of a `RegionPlan` from the reachable blocks, the natural
//! loops, and the structurer's try context: one slot in the enclosing
//! region's topological order per direct child (a natural loop, a try unit,
//! or a plain block).

use super::try_context::TryContext;
use super::{NaturalLoop, RegionPlan, UnitPlan, block_span, successors, wasm_error};
use crate::BackendError;
use crate::mir::{BasicBlock, BlockId, Function};
use std::collections::{HashMap, HashSet};

/// One direct child of a region plan: a natural loop or a try unit.
#[derive(Clone, Copy, Debug)]
enum RegionChild<'a> {
    Loop(&'a NaturalLoop),
    Try(BlockId),
}

pub(super) fn build_region(
    function: &Function,
    blocks: &HashMap<BlockId, &BasicBlock>,
    members: &HashSet<BlockId>,
    loops: &[NaturalLoop],
    header: Option<BlockId>,
    entry: BlockId,
    tries: &TryContext,
) -> Result<RegionPlan, Vec<BackendError>> {
    let children = direct_children(function, members, loops, tries, header)?;
    // The full claim set per child: a loop owns its members plus the claims
    // of the try units it contains (the "try inside loop" shape); a try unit
    // owns its transitive claim — its extent and handlers plus every nested
    // try's claim — all of which it emits inline and which is therefore never
    // a unit of its own.
    let child_blocks = children
        .iter()
        .map(|child| match child {
            RegionChild::Loop(loop_) => {
                let mut claimed = loop_.blocks.clone();
                for region in &tries.regions.regions {
                    if loop_.blocks.contains(&region.try_block) {
                        claimed.extend(tries.claim(region.try_block).iter().copied());
                    }
                }
                claimed
            }
            RegionChild::Try(try_block) => tries.claim(*try_block).clone(),
        })
        .collect::<Vec<_>>();

    let mut owner = HashMap::<BlockId, usize>::new();
    for (index, claimed) in child_blocks.iter().enumerate() {
        for block in claimed {
            if owner.insert(*block, index).is_some() {
                return Err(wasm_error(
                    function.span,
                    "MIR region plan claims a block twice; the control flow is irreducible",
                ));
            }
        }
    }

    let mut units = Vec::<UnitPlan>::new();
    let mut unit_blocks = Vec::<HashSet<BlockId>>::new();
    let mut unit_for_block = HashMap::<BlockId, usize>::new();
    for (index, child) in children.iter().enumerate() {
        let claimed = child_blocks[index].clone();
        match child {
            RegionChild::Loop(loop_) => {
                units.push(UnitPlan::Loop {
                    header: loop_.header,
                    body: Box::new(build_region(
                        function,
                        blocks,
                        &loop_.blocks,
                        loops,
                        Some(loop_.header),
                        loop_.header,
                        tries,
                    )?),
                });
            }
            RegionChild::Try(try_block) => {
                let region = tries.region(*try_block);
                let uniform = tries.uniform(*try_block);
                // The protected sub-region is the raw dominated set: inner
                // tries and loops reappear there as nested child units.
                let protected = Box::new(build_region(
                    function,
                    blocks,
                    &region.raw,
                    loops,
                    None,
                    region.entry,
                    tries,
                )?);
                units.push(UnitPlan::Try {
                    try_block: *try_block,
                    protected,
                    handler: uniform.map(|clauses| clauses.handler),
                    payload: uniform.and_then(|clauses| clauses.payload),
                });
            }
        }
        unit_blocks.push(claimed.clone());
        for block in &claimed {
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
    function: &Function,
    members: &HashSet<BlockId>,
    loops: &'a [NaturalLoop],
    tries: &TryContext,
    header: Option<BlockId>,
) -> Result<Vec<RegionChild<'a>>, Vec<BackendError>> {
    let mut candidates: Vec<RegionChild<'a>> = loops
        .iter()
        .filter(|loop_| {
            Some(loop_.header) != header
                && loop_.blocks.is_subset(members)
                && (header.is_none() || loop_.blocks.len() < members.len())
        })
        .map(RegionChild::Loop)
        .collect();
    // A try unit is a candidate of the region that contains its try block:
    // that is where the unit is emitted, and its claim may reach past a
    // containing loop's blocks (the "try inside loop" shape).
    for region in &tries.regions.regions {
        if members.contains(&region.try_block) {
            candidates.push(RegionChild::Try(region.try_block));
        }
    }
    // A candidate strictly contained in another candidate belongs to that
    // child's sub-region; only the maximal candidates are direct children.
    // Containment is by the extent a candidate's emission covers: a loop
    // contains a try whose try block it holds, a try contains a loop inside
    // its raw dominated set, and equal extents are two headers for the same
    // blocks.
    let mut children = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let mut nested = false;
        for (other_index, other) in candidates.iter().enumerate() {
            if other_index == index {
                continue;
            }
            if contained_in(function, candidate, other, tries)? {
                nested = true;
                break;
            }
        }
        if !nested {
            children.push(*candidate);
        }
    }
    Ok(children)
}

/// Whether `candidate` is emitted inside `other`'s sub-region. Returns an
/// error when the two candidates cover exactly the same blocks: two headers
/// for one region cannot both be emitted.
fn contained_in(
    function: &Function,
    candidate: &RegionChild<'_>,
    other: &RegionChild<'_>,
    tries: &TryContext,
) -> Result<bool, Vec<BackendError>> {
    let (same, nested) = match (candidate, other) {
        (RegionChild::Loop(inner), RegionChild::Loop(outer)) => (
            inner.blocks == outer.blocks,
            inner.blocks.is_subset(&outer.blocks) && inner.blocks.len() < outer.blocks.len(),
        ),
        (RegionChild::Try(inner), RegionChild::Try(outer)) => (
            tries.extent(*inner) == tries.extent(*outer),
            tries.extent(*inner).is_subset(tries.extent(*outer))
                && tries.extent(*inner).len() < tries.extent(*outer).len(),
        ),
        (RegionChild::Try(inner), RegionChild::Loop(outer)) => (
            false,
            outer.blocks.contains(&tries.region(*inner).try_block),
        ),
        (RegionChild::Loop(inner), RegionChild::Try(outer)) => {
            (false, inner.blocks.is_subset(&tries.region(*outer).raw))
        }
    };
    if same {
        return Err(wasm_error(
            function.span,
            "MIR region plan has two headers for the same blocks; the control flow is irreducible",
        ));
    }
    Ok(nested)
}
