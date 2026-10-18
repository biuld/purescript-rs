//! The import symbols a lowered MIR module references: the one place P9 and
//! the optimizer consult shared call/list-copy reachability.
use crate::mir::{Function, Instruction, Terminator};
use psrs_hir::SymbolId;
use std::collections::HashSet;

/// Whether a list element carries byte-list payloads that need the string
/// boundary helpers.
pub fn element_has_bytes(element: &crate::abi::canonical::CanonicalType) -> bool {
    use crate::abi::canonical::CanonicalType;
    match element {
        CanonicalType::String => true,
        CanonicalType::List(inner) | CanonicalType::FixedList { element: inner, .. } => {
            element_has_bytes(inner)
        }
        CanonicalType::Record(fields) => fields
            .iter()
            .any(|field| field.ty.is_byte_list() || element_has_bytes(&field.ty)),
        CanonicalType::Option(inner) => element_has_bytes(inner),
        CanonicalType::Result { ok, err } => {
            ok.as_deref().is_some_and(element_has_bytes)
                || err.as_deref().is_some_and(element_has_bytes)
        }
        CanonicalType::Variant(cases) => cases
            .iter()
            .any(|case| case.payload.as_deref().is_some_and(element_has_bytes)),
        _ => false,
    }
}

/// The import symbols referenced by any call in the module.
pub fn referenced_imports(functions: &[Function]) -> HashSet<SymbolId> {
    let mut used = HashSet::new();
    for function in functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                match instruction {
                    Instruction::Call { function, .. } | Instruction::CallVoid { function, .. } => {
                        used.insert(*function);
                    }
                    Instruction::ListCopy { element, .. } if element_has_bytes(element) => {
                        used.insert(crate::abi::STRING_TO_BYTES_SYMBOL);
                        used.insert(crate::abi::BYTES_TO_STRING_SYMBOL);
                        used.insert(crate::abi::REALLOC_SYMBOL);
                    }
                    _ => {}
                }
            }
            if let Some(Terminator::ReturnCall { function, .. }) = &block.terminator {
                used.insert(*function);
            }
        }
    }
    used
}
