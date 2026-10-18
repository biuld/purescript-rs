//! The try-region partition: one definition of a `Try` terminator's protected
//! region, shared by MIR verification (`verify/exception.rs`), tail-call
//! marking (`lower/tail.rs`), and the Wasm structurer's try units. See the
//! design's "Try-region partition" algorithm in
//! `docs/design/backend/fp/exception-handling-and-stack-exhaustion.md`.
//!
//! The protected region of a `Try` (try block `t`, protected entry `p`) is the
//! set of blocks dominated by `p` over the normal CFG — the `Try` terminator
//! contributes the ordinary edge `t -> p` — minus the protected regions of
//! inner tries, minus every handler block. Dominance is a tree order, so raw
//! regions are nested or disjoint by construction; the carve-out turns nested
//! raw regions into sibling claims. The verifier's nesting check guards that
//! invariant.

use super::cfg;
use super::{BlockId, Function, Terminator};
use crate::BackendError;
use crate::mir::tag::TagId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

/// One `Try` terminator's region.
pub(crate) struct TryRegion {
    /// The block whose terminator is the `Try`. The Wasm structurer's try
    /// unit (EH-10) emits it immediately before the synthesized unwind block.
    pub try_block: BlockId,
    /// The first protected block; control enters it normally from `try_block`.
    pub entry: BlockId,
    pub catches: Vec<(TagId, BlockId)>,
    pub catch_all: Option<BlockId>,
    /// Blocks dominated by `entry`, minus the protected regions of inner
    /// tries, minus every handler block.
    pub protected: HashSet<BlockId>,
    /// The raw dominated set, before the carve-outs by inner tries and by
    /// handler exclusion. The structurer builds the try unit's protected
    /// sub-region from this set — inner tries reappear there as nested
    /// units — and checks nesting of try regions against natural loops with
    /// it.
    pub raw: HashSet<BlockId>,
    pub span: TextRange,
}

impl TryRegion {
    /// Every clause handler, in clause order plus `catch_all`.
    pub(crate) fn handlers(&self) -> Vec<BlockId> {
        self.catches
            .iter()
            .map(|(_, handler)| *handler)
            .chain(self.catch_all)
            .collect()
    }

    /// The handler the region assigns to a throw with `tag`: the matching
    /// tag clause's handler, else the `catch_all` handler, else `None`.
    pub(crate) fn matching_handler(&self, tag: TagId) -> Option<BlockId> {
        self.catches
            .iter()
            .find(|(clause_tag, _)| *clause_tag == tag)
            .map(|(_, handler)| *handler)
            .or(self.catch_all)
    }
}

/// The partition of a function's blocks into try regions.
pub(crate) struct TryRegions {
    /// Innermost protected regions first (smallest raw dominated set first;
    /// the sort is stable, so equal raw sets keep declaration order). Every
    /// consumer that needs the nesting order reconstructs it by set
    /// containment over `raw`.
    pub regions: Vec<TryRegion>,
    /// Every clause handler mapped to its owning try block.
    pub owners: HashMap<BlockId, BlockId>,
    /// Handler blocks claimed by a second `Try`, paired with that try's span.
    pub conflicts: Vec<(BlockId, TextRange)>,
}

impl TryRegions {
    /// The region whose terminator is the given try block.
    pub(crate) fn region(&self, try_block: BlockId) -> Option<&TryRegion> {
        self.regions
            .iter()
            .find(|region| region.try_block == try_block)
    }
    /// Every protected block, across all regions.
    pub(crate) fn protected_blocks(&self) -> HashSet<BlockId> {
        self.regions
            .iter()
            .flat_map(|region| region.protected.iter().copied())
            .collect()
    }

    /// Every handler block named by any clause of any region.
    pub(crate) fn handler_blocks(&self) -> HashSet<BlockId> {
        self.owners.keys().copied().collect()
    }
}

/// Computes the partition and its structural checks. Fails with the offending
/// terminator's span when a region's blocks do not exist, when protected
/// regions overlap without nesting, or when a back edge targets a try block.
pub(crate) fn plan(
    function: &Function,
    pass: &'static str,
) -> Result<TryRegions, Vec<BackendError>> {
    let tries = try_blocks(function);
    if tries.is_empty() {
        return Ok(TryRegions {
            regions: Vec::new(),
            owners: HashMap::new(),
            conflicts: Vec::new(),
        });
    }
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    let try_blocks = tries
        .iter()
        .map(|(try_block, ..)| *try_block)
        .collect::<HashSet<_>>();

    // Handler ownership: each clause handler belongs to exactly one `Try`.
    let mut owners = HashMap::<BlockId, BlockId>::new();
    let mut conflicts = Vec::new();
    for (try_block, terminator, span) in &tries {
        for handler in clause_handlers(terminator) {
            if owners.insert(handler, *try_block).is_some() {
                conflicts.push((handler, *span));
            }
        }
    }

    // Existence of the blocks the terminator names, before dominance runs.
    let mut errors = Vec::new();
    let mut valid = Vec::new();
    for (try_block, terminator, span) in &tries {
        let Terminator::Try {
            protected,
            catches,
            catch_all,
            ..
        } = terminator
        else {
            continue;
        };
        if !blocks.contains_key(protected) {
            errors.push(invalid(
                pass,
                *span,
                "MIR try region's protected entry block does not exist",
            ));
            continue;
        }
        if clause_handlers(terminator)
            .iter()
            .any(|handler| !blocks.contains_key(handler))
        {
            errors.push(invalid(
                pass,
                *span,
                "MIR try clause handler block does not exist",
            ));
            continue;
        }
        valid.push((*try_block, *protected, catches.clone(), *catch_all, *span));
    }

    // Dominance over the CFG that includes unwind edges (the design's
    // invariant section rechecks the partition on that CFG): the handler edge
    // keeps a join shared by the protected and catch paths out of the raw
    // dominated set, so the region protects only what the `try_table` covers.
    let dominators = cfg::dominators_with_unwind_edges(function);
    // Raw regions: blocks dominated by the protected entry. Innermost first
    // (the stable sort keeps declaration order for equal sizes), so the
    // carve-out below always sees an inner try's final claim.
    let mut ordered = valid
        .iter()
        .map(|(try_block, entry, catches, catch_all, span)| {
            let raw = function
                .blocks
                .iter()
                .filter(|block| {
                    dominators
                        .get(&block.id)
                        .is_some_and(|dominators| dominators.contains(entry))
                })
                .map(|block| block.id)
                .collect::<HashSet<_>>();
            (*try_block, *entry, catches.clone(), *catch_all, *span, raw)
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(_, _, _, _, _, raw)| raw.len());

    let mut computed: Vec<(BlockId, HashSet<BlockId>)> = Vec::new();
    let mut regions = Vec::new();
    for (try_block, entry, catches, catch_all, span, raw) in ordered {
        let mut protected = raw.clone();
        // Carve out every already-computed inner region's claim. Equal raw
        // sets are one region claimed by two tries; neither carves the other,
        // and the equal sets satisfy the nesting check as written.
        for (_, done) in &computed {
            if done.is_subset(&raw) && *done != raw {
                protected.retain(|block| !done.contains(block));
            }
        }
        protected.retain(|block| !owners.contains_key(block));
        computed.push((try_block, protected.clone()));
        regions.push(TryRegion {
            try_block,
            entry,
            catches,
            catch_all,
            protected,
            raw,
            span,
        });
    }

    // Protected regions of distinct tries are nested or are disjoint.
    for (index, left) in regions.iter().enumerate() {
        for right in &regions[index + 1..] {
            if left.protected.is_disjoint(&right.protected) {
                continue;
            }
            if !left.protected.is_subset(&right.protected)
                && !right.protected.is_subset(&left.protected)
            {
                errors.push(invalid(
                    pass,
                    right.span,
                    "MIR try regions overlap without nesting; the control flow is irreducible",
                ));
            }
        }
    }

    // No back edge targets a try block.
    for block in &function.blocks {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        for target in cfg::successors(terminator) {
            if try_blocks.contains(&target)
                && dominators
                    .get(&block.id)
                    .is_some_and(|dominators| dominators.contains(&target))
            {
                errors.push(invalid(
                    pass,
                    terminator_span(terminator),
                    "MIR back edge targets a try block",
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(TryRegions {
            regions,
            owners,
            conflicts,
        })
    } else {
        Err(errors)
    }
}

/// The handler blocks named by any try clause, catch_all, or throw, without
/// the partition's dominance work. Passes that must not drop handler blocks
/// (reachability pruning) use this; the partition derives the same set from
/// the clauses it plans.
pub(crate) fn handler_closure(function: &Function) -> HashSet<BlockId> {
    let mut handlers = HashSet::new();
    for block in &function.blocks {
        let Some(terminator) = &block.terminator else {
            continue;
        };
        match terminator {
            Terminator::Try {
                catches, catch_all, ..
            } => {
                for (_, handler) in catches {
                    handlers.insert(*handler);
                }
                if let Some(handler) = catch_all {
                    handlers.insert(*handler);
                }
            }
            Terminator::Throw {
                handler: Some(handler),
                ..
            } => {
                handlers.insert(*handler);
            }
            _ => {}
        }
    }
    handlers
}

fn try_blocks(function: &Function) -> Vec<(BlockId, &Terminator, TextRange)> {
    function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            Some(terminator @ Terminator::Try { span, .. }) => Some((block.id, terminator, *span)),
            _ => None,
        })
        .collect()
}

fn clause_handlers(terminator: &Terminator) -> Vec<BlockId> {
    let Terminator::Try {
        catches, catch_all, ..
    } = terminator
    else {
        return Vec::new();
    };
    let mut handlers = catches
        .iter()
        .map(|(_, handler)| *handler)
        .collect::<Vec<_>>();
    if let Some(handler) = catch_all {
        handlers.push(*handler);
    }
    handlers
}

fn terminator_span(terminator: &Terminator) -> TextRange {
    match terminator {
        Terminator::Trap { span }
        | Terminator::Return { span, .. }
        | Terminator::Jump { span, .. }
        | Terminator::Branch { span, .. }
        | Terminator::Switch { span, .. }
        | Terminator::ReturnCall { span, .. }
        | Terminator::ReturnCallRef { span, .. }
        | Terminator::Throw { span, .. }
        | Terminator::Try { span, .. } => *span,
    }
}

fn invalid(pass: &'static str, span: TextRange, message: &'static str) -> BackendError {
    BackendError::invalid_ir(pass, span, message)
}
