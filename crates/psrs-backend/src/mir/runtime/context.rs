//! One checked runtime binding inventory shared by all function lowerers.
use super::*;
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(in crate::mir) struct RuntimeContext {
    pub(in crate::mir) imports: HashMap<SymbolId, Import>,
}

impl RuntimeContext {
    pub(in crate::mir) fn checked(
        bindings: &[RuntimeBinding],
        physical: &cc::Module,
        layout: &PlannedLayout,
    ) -> Result<Self, Vec<BackendError>> {
        let live = ReachableHandles::from_module(physical).map_err(|failure| {
            vec![BackendError::invalid_ir(
                "P9 runtime projection",
                physical.span,
                failure.to_string(),
            )]
        })?;
        // A live source runtime binding needs the withdrawn State contract to
        // map its operands. There is no generic replacement, so planning it is
        // an explicit failure rather than a fabricated successful binding.
        for binding in bindings {
            if live.direct_calls.contains(&binding.symbol) {
                plan_import(binding, physical, layout)?;
            }
        }
        Ok(Self {
            imports: HashMap::new(),
        })
    }
}
