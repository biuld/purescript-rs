//! One physical schema per static binding, shared by verification and encoding.
use super::{Instruction, Module};
use crate::{BackendError, types::DefinedTypeId};
use psrs_hir::SymbolId;
use std::collections::HashMap;

pub(crate) fn collect(
    module: &Module,
) -> Result<HashMap<SymbolId, DefinedTypeId>, Vec<BackendError>> {
    let mut cells = HashMap::new();
    for instruction in module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
    {
        if let Instruction::StaticCell {
            binding,
            type_index,
            span,
            ..
        } = instruction
            && cells
                .insert(*binding, *type_index)
                .is_some_and(|old| old != *type_index)
        {
            return Err(vec![BackendError::invalid_ir(
                "P9 static storage",
                *span,
                "static binding has inconsistent array layouts",
            )]);
        }
    }
    Ok(cells)
}
