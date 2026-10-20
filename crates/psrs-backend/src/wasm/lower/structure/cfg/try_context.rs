//! The structurer's checked view of the function's try regions (EH-10). The
//! partition itself is `mir/try_region`; this context adds the
//! structuring-level rules of the design's "Structuring: what `try_table`
//! adds": all clauses of one region must agree on the payload and handler
//! they deliver to (one synthesized unwind block cannot distinguish them),
//! and a try region may nest inside, contain, or stay disjoint from every
//! natural loop, but never straddle one.

use super::{NaturalLoop, wasm_error};
use crate::BackendError;
use crate::mir::try_region::{self, TryRegions};
use crate::mir::{BlockId, Tag};
use crate::types::ValueType;
use std::collections::{HashMap, HashSet};

pub(super) struct TryContext {
    pub(super) regions: TryRegions,
    uniform: HashMap<BlockId, UniformClauses>,
    /// Each try unit's full extent: the raw dominated set plus the try block.
    extents: HashMap<BlockId, HashSet<BlockId>>,
    /// Each try unit's transitive claim: its extent and handlers plus, for
    /// every try nested inside its protected body, that try's whole claim —
    /// the nested unit emits all of it inline, so no ancestor may emit it
    /// again.
    claims: HashMap<BlockId, HashSet<BlockId>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct UniformClauses {
    pub(super) payload: Option<ValueType>,
    pub(super) handler: BlockId,
}

impl TryContext {
    pub(super) fn new(
        regions: TryRegions,
        tags: &[Tag],
        loops: &[NaturalLoop],
    ) -> Result<Self, Vec<BackendError>> {
        let mut uniform = HashMap::new();
        let mut extents = HashMap::new();
        let mut claims = HashMap::new();
        for region in &regions.regions {
            match uniform_clauses(region, tags) {
                Some(clauses) => {
                    uniform.insert(region.try_block, clauses);
                }
                None => {
                    return Err(wasm_error(
                        region.span,
                        "MIR try region's catch clauses deliver different payloads or handlers; the structured encoding has one unwind block for all of them",
                    ));
                }
            }
            let mut extent = region.raw.clone();
            extent.insert(region.try_block);
            extents.insert(region.try_block, extent.clone());
            // The base claim: the unit's own extent plus every clause handler,
            // which it emits inline after the unwind block.
            let mut claim = extent;
            claim.extend(region.handlers());
            claims.insert(region.try_block, claim);
        }
        // Close the claims: a try nested inside another's protected body (its
        // try block lies in the outer raw dominated set) is emitted inline by
        // the outer unit, so the outer claim absorbs the inner one.
        loop {
            let mut changed = false;
            for region in &regions.regions {
                for nested in &regions.regions {
                    if nested.try_block == region.try_block
                        || !region.raw.contains(&nested.try_block)
                    {
                        continue;
                    }
                    let size = claims[&region.try_block].len();
                    let nested_claim = claims[&nested.try_block].clone();
                    claims
                        .get_mut(&region.try_block)
                        .expect("every region has a base claim")
                        .extend(nested_claim);
                    changed |= claims[&region.try_block].len() != size;
                }
            }
            if !changed {
                break;
            }
        }
        validate_try_loop_nesting(&regions, loops)?;
        Ok(Self {
            regions,
            uniform,
            extents,
            claims,
        })
    }

    /// The try region behind a try block.
    pub(super) fn region(&self, try_block: BlockId) -> &try_region::TryRegion {
        self.regions
            .region(try_block)
            .expect("the context was built from the same partition")
    }

    /// The clause projection a try unit emits.
    pub(super) fn uniform(&self, try_block: BlockId) -> Option<UniformClauses> {
        self.uniform.get(&try_block).copied()
    }

    /// The try unit's full block extent.
    pub(super) fn extent(&self, try_block: BlockId) -> &HashSet<BlockId> {
        self.extents
            .get(&try_block)
            .expect("the context was built from the same partition")
    }

    /// Every block the try unit emits, transitively over nested tries.
    pub(super) fn claim(&self, try_block: BlockId) -> &HashSet<BlockId> {
        self.claims
            .get(&try_block)
            .expect("the context was built from the same partition")
    }
}

/// Projects a region's clauses onto the synthesized unwind block: every
/// clause delivers the same optional payload to the same handler. A region
/// with no clauses arms nothing and emits no handler.
fn uniform_clauses(region: &try_region::TryRegion, tags: &[Tag]) -> Option<UniformClauses> {
    let mut projected = Vec::new();
    for clause in &region.clauses {
        let payload = match clause {
            crate::mir::CatchClause::Catch { tag, .. } => tags
                .get(tag.0 as usize)
                .and_then(|tag| tag.payload.first().copied()),
            crate::mir::CatchClause::CatchAll { .. } => None,
            crate::mir::CatchClause::CatchAllRef { .. } => {
                Some(ValueType::Ref(crate::types::RefType {
                    nullable: false,
                    heap: crate::types::HeapType::Exn,
                }))
            }
        };
        projected.push((payload, clause.target()));
    }
    let mut clauses = projected.iter();
    let first = *clauses.next()?;
    if clauses.any(|clause| clause != &first) {
        return None;
    }
    Some(UniformClauses {
        payload: first.0,
        handler: first.1,
    })
}

/// A try region may nest inside a natural loop ("try inside loop"), contain
/// one ("loop inside protected region"), or stay disjoint. A region whose try
/// block sits in the loop is emitted inside the loop's body, and its claim may
/// reach past the loop's blocks (blocks the loop's back edge cannot reach);
/// everything else that overlaps a loop without nesting straddles it and
/// cannot be emitted inside a structured `try_table` body.
fn validate_try_loop_nesting(
    regions: &TryRegions,
    loops: &[NaturalLoop],
) -> Result<(), Vec<BackendError>> {
    for region in &regions.regions {
        let mut extent = region.raw.clone();
        extent.insert(region.try_block);
        for loop_ in loops {
            if extent.is_disjoint(&loop_.blocks)
                || extent.is_subset(&loop_.blocks)
                || loop_.blocks.is_subset(&region.raw)
                || loop_.blocks.contains(&region.try_block)
            {
                continue;
            }
            return Err(wasm_error(
                region.span,
                "MIR try region overlaps a natural loop without nesting; the control flow is irreducible",
            ));
        }
    }
    Ok(())
}
