//! Allocates callable symbols for functions generated during closure conversion.

use psrs_core::Module as CoreModule;
use psrs_hir::{ModuleId, SymbolId};
use std::collections::{HashMap, HashSet};

/// Allocates generated callable symbols without relying on source offsets.
///
/// Linked Core keeps source declaration symbols from each input module but
/// lowers generated functions after linking. A shared allocator therefore
/// reserves every existing callable symbol and allocates within the
/// originating source module so diagnostics retain their source ownership.
pub(in crate::cc) struct GeneratedSymbolAllocator {
    used: HashSet<SymbolId>,
    next: HashMap<ModuleId, u32>,
}

impl GeneratedSymbolAllocator {
    pub(in crate::cc) fn new(module: &CoreModule) -> Self {
        let used = module
            .declarations
            .iter()
            .map(|declaration| declaration.symbol)
            .chain(module.externals.iter().map(|external| external.symbol))
            .collect();
        Self {
            used,
            next: HashMap::new(),
        }
    }

    pub(in crate::cc) fn fresh(&mut self, module: ModuleId) -> SymbolId {
        let next = self.next.entry(module).or_default();
        loop {
            let symbol = SymbolId::new(module, *next);
            *next = next
                .checked_add(1)
                .expect("generated symbol index space exhausted");
            if self.used.insert(symbol) {
                return symbol;
            }
        }
    }
}
