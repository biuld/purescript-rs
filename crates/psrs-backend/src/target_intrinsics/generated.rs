//! Reserved generated bindings and their concrete MIR contracts.
use crate::types::{CompositeType, DefinedTypeId, HeapType, RefType, StorageType, ValueType};
use crate::{abi, mir};
use psrs_hir::SymbolId;

pub(crate) fn name(symbol: SymbolId) -> Option<&'static str> {
    match symbol {
        abi::REALLOC_SYMBOL => Some("realloc"),
        abi::STRING_TO_BYTES_SYMBOL => Some("string_to_bytes"),
        abi::BYTES_TO_STRING_SYMBOL => Some("bytes_to_string"),
        abi::VALIDATE_STEP_SYMBOL => Some("validate_step"),
        _ => None,
    }
}

/// Shared by the planner and the helper emitter, independent of a consumer call.
pub(crate) fn signature(symbol: SymbolId, string: Option<DefinedTypeId>) -> Option<mir::Import> {
    let string = || {
        string.map(|index| {
            ValueType::Ref(RefType {
                nullable: false,
                heap: HeapType::Index(index),
            })
        })
    };
    let (parameters, result) = match symbol {
        abi::REALLOC_SYMBOL => (vec![ValueType::I32; 4], ValueType::I32),
        abi::STRING_TO_BYTES_SYMBOL => (vec![string()?], ValueType::I32),
        abi::BYTES_TO_STRING_SYMBOL => (vec![ValueType::I32; 2], string()?),
        abi::VALIDATE_STEP_SYMBOL => (vec![ValueType::I32; 2], ValueType::I32),
        _ => return None,
    };
    Some(mir::Import {
        symbol,
        parameters,
        result: Some(result),
    })
}

pub(crate) fn verify(module: &mir::Module, import: &mir::Import) -> Result<(), String> {
    let string = string_type_from_imports(module);
    let expected = signature(import.symbol, string)
        .ok_or("generated helper has no concrete string representation")?;
    if *import != expected {
        return Err("generated helper consumer signature differs from its provider".into());
    }
    if matches!(
        import.symbol,
        abi::STRING_TO_BYTES_SYMBOL | abi::BYTES_TO_STRING_SYMBOL | abi::VALIDATE_STEP_SYMBOL
    ) {
        let index = string.ok_or("generated codec has no string type")?;
        let ty = module
            .types
            .iter()
            .flat_map(|group| &group.0)
            .nth(index.0 as usize)
            .ok_or("generated codec references an unknown string type")?;
        if !matches!(ty.composite, CompositeType::Array(field)
            if field.mutable && field.storage == StorageType::I8)
        {
            return Err("generated codec requires a mutable GC byte array".into());
        }
    }
    Ok(())
}

/// The GC string defined-type index, read from a reserved helper import.
pub(crate) fn string_type_from_imports(module: &crate::mir::Module) -> Option<DefinedTypeId> {
    for import in &module.imports {
        if import.symbol != crate::abi::STRING_TO_BYTES_SYMBOL
            && import.symbol != crate::abi::BYTES_TO_STRING_SYMBOL
        {
            continue;
        }
        for ty in import.parameters.iter().chain(import.result.iter()) {
            if let ValueType::Ref(reference) = ty
                && let HeapType::Index(index) = reference.heap
            {
                return Some(index);
            }
        }
    }
    None
}
