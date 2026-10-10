//! One checked runtime binding inventory shared by all function lowerers.
use super::*;
use psrs_hir::SymbolId;
use std::{collections::HashMap, sync::Arc};

pub(in crate::mir) struct RuntimeContext {
    pub(in crate::mir) logical: Arc<cc::Module>,
    bindings: HashMap<SymbolId, RuntimeBinding>,
    pub(in crate::mir) imports: HashMap<SymbolId, Import>,
}

impl RuntimeContext {
    pub(in crate::mir) fn checked(
        bindings: &[RuntimeBinding],
        logical: Arc<cc::Module>,
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
        let mut imports = HashMap::new();
        let mut selected = HashMap::new();
        for binding in bindings {
            if !live.direct_calls.contains(&binding.symbol) {
                continue;
            }
            let (import, _) = plan_import(binding, &logical, layout)?;
            verify_import(&import, &layout.types).map_err(|message| error(binding, &message))?;
            imports.insert(binding.symbol, import);
            selected.insert(binding.symbol, binding.clone());
        }
        Ok(Self {
            logical,
            bindings: selected,
            imports,
        })
    }

    pub(in crate::mir) fn binding(&self, symbol: SymbolId) -> Option<&RuntimeBinding> {
        self.bindings.get(&symbol)
    }
}
