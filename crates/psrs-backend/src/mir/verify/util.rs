//! Shared helpers for MIR verification: value lookup, reference and storage
//! classification, and diagnostics.

use crate::BackendError;
use crate::mir::{Function, ValueId, ValueType};
use crate::types::{CompositeType, DefinedType, HeapType, StorageType};
use psrs_span::TextRange;
use std::collections::HashMap;

pub(super) fn composite_at<'a>(
    defined: &[&'a DefinedType],
    index: u32,
) -> Option<&'a CompositeType> {
    defined.get(index as usize).map(|def| &def.composite)
}

/// The low-level value type a struct or array field stores, when representable.
pub(super) fn storage_value_type(storage: &StorageType) -> Option<ValueType> {
    Some(match storage {
        StorageType::I8 | StorageType::I16 | StorageType::I32 => ValueType::I32,
        StorageType::I64 => ValueType::I64,
        StorageType::F32 => ValueType::F32,
        StorageType::F64 => ValueType::F64,
        StorageType::Ref(reference) => ValueType::Ref(*reference),
        StorageType::V128 => return None,
    })
}

pub(super) fn value_type(function: &Function, value: ValueId) -> Option<ValueType> {
    function
        .values
        .iter()
        .find(|decl| decl.id == value)
        .map(|decl| decl.ty)
}

pub(super) fn require_value(
    definitions: &HashMap<ValueId, ValueType>,
    value: ValueId,
    span: TextRange,
) -> Result<ValueType, Vec<BackendError>> {
    definitions
        .get(&value)
        .copied()
        .ok_or_else(|| mir_error(span, "MIR instruction uses an unknown value"))
}

pub(super) fn check_heap(
    heap: HeapType,
    defined: &[&DefinedType],
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if let HeapType::Index(index) = heap
        && index as usize >= defined.len()
    {
        return Err(mir_error(
            span,
            "MIR defined type reference is out of range",
        ));
    }
    Ok(())
}

pub(super) fn is_ref(ty: ValueType) -> bool {
    matches!(ty, ValueType::Ref(_))
}

pub(super) fn is_ref_opt(ty: Option<ValueType>) -> bool {
    matches!(ty, Some(ValueType::Ref(_)))
}

pub(super) fn mir_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P9 MIR verification", span, message)]
}
