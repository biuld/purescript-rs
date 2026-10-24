//! Deterministic per-instance GC globals for checked static bindings.
use super::*;
pub(super) struct StaticGlobals {
    pub globals: Vec<Global>,
    pub indices: HashMap<SymbolId, GlobalIndex>,
}
pub(super) fn collect(
    module: &mir::Module,
    base: usize,
) -> Result<StaticGlobals, Vec<BackendError>> {
    let mut cells: Vec<_> = mir::static_cells::collect(module)?.into_iter().collect();
    cells.sort_by_key(|(symbol, _)| (symbol.module.0, symbol.index));
    let mut globals = Vec::new();
    let mut indices = HashMap::new();
    for (binding, ty) in cells {
        let index = GlobalIndex((base + globals.len()) as u32);
        let heap = wasm_encoder::HeapType::Concrete(ty.0);
        globals.push(Global {
            index,
            mutable: true,
            ty: ValType::Ref(RefType {
                nullable: true,
                heap_type: heap,
            }),
            init: GlobalInit::RefNull(heap),
        });
        indices.insert(binding, index);
    }
    Ok(StaticGlobals { globals, indices })
}
