//! Checked physical view of a source static-cell binding, discharged in P9.
use super::*;
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(super) fn plan(
    binding: &RuntimeBinding,
    module: &cc::Module,
    layout: &PlannedLayout,
    cells: &mut HashMap<SymbolId, Import>,
) -> Result<(), Vec<BackendError>> {
    crate::bindings::check_runtime_call(binding, module)?;
    let signature = module
        .externals
        .iter()
        .find(|external| external.symbol == binding.symbol)
        .and_then(|external| external.signature.as_ref())
        .expect("checked external signature");
    let result = layout.value_type(&signature.result).map_err(|_| {
        vec![
            BackendError::invalid_ir(
                "P9 static storage",
                binding.span,
                "static cell has no array layout",
            )
            .with_module(binding.source_module),
        ]
    })?;
    cells.insert(
        binding.symbol,
        Import {
            symbol: binding.symbol,
            runtime: None,
            parameters: vec![crate::types::ValueType::Ref(crate::types::RefType {
                nullable: true,
                heap: crate::types::HeapType::Eq,
            })],
            result: Some(result),
        },
    );
    Ok(())
}
