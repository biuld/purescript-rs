//! One conservative normal/unwind graph, including opaque reference throws.
use super::{BlockId, CatchClause, Function, Terminator, cfg, try_region};
use crate::BackendError;
use std::collections::{HashMap, HashSet};

pub(crate) struct UnwindGraph {
    edges: HashMap<BlockId, Vec<BlockId>>,
    escaping: HashSet<BlockId>,
}

impl UnwindGraph {
    pub(crate) fn new(function: &Function, pass: &'static str) -> Result<Self, Vec<BackendError>> {
        let regions = try_region::plan(function, pass)?;
        Ok(Self::with_regions(function, &regions))
    }

    pub(crate) fn with_regions(function: &Function, regions: &try_region::TryRegions) -> Self {
        let mut edges = HashMap::new();
        let mut escaping = HashSet::new();
        for block in &function.blocks {
            let mut next = Vec::new();
            if let Some(terminator) = &block.terminator {
                next = cfg::unwind_successors(terminator);
                if matches!(terminator, Terminator::ThrowRef { .. }) {
                    let mut caught_tags = HashSet::new();
                    let mut catches_all = false;
                    'outer: for region in regions.enclosing(block.id) {
                        for clause in &region.clauses {
                            match clause {
                                CatchClause::Catch { tag, target } => {
                                    if caught_tags.insert(*tag) {
                                        next.push(*target);
                                    }
                                }
                                CatchClause::CatchAll { target }
                                | CatchClause::CatchAllRef { target } => {
                                    next.push(*target);
                                    catches_all = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                    if !catches_all {
                        escaping.insert(block.id);
                    }
                }
                if matches!(terminator, Terminator::Throw { handler: None, .. }) {
                    escaping.insert(block.id);
                }
            }
            next.sort_by_key(|block| block.0);
            next.dedup();
            edges.insert(block.id, next);
        }
        Self { edges, escaping }
    }

    pub(crate) fn successors(&self, block: BlockId) -> &[BlockId] {
        &self.edges[&block]
    }

    pub(crate) fn escapes(&self, block: BlockId) -> bool {
        self.escaping.contains(&block)
    }
}
