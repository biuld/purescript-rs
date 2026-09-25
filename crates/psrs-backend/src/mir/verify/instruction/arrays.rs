//! Array instruction checks for MIR verification.

use super::super::util::{
    composite_at, is_any_array_reference, is_array_reference, mir_error, require_value, value_type,
};
use crate::BackendError;
use crate::mir::{Function, Instruction, ValueId, ValueType};
use crate::types::{CompositeType, DefinedType, DefinedTypeId, HeapType, StorageType};
use std::collections::HashMap;

pub(super) fn verify_array_new_default(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::ArrayNewDefault {
        destination,
        type_index,
        length,
        source,
        index,
        span,
        ..
    } = instruction
    else {
        unreachable!("array allocation verifier received another instruction")
    };
    if !matches!(
        composite_at(defined, *type_index),
        Some(CompositeType::Array(_))
    ) {
        return Err(mir_error(
            *span,
            "MIR array.new_default type is not an array",
        ));
    }
    let Some(CompositeType::Array(element)) = composite_at(defined, *type_index) else {
        unreachable!("array type was checked above")
    };
    if !is_defaultable_storage(&element.storage) {
        return Err(mir_error(
            *span,
            "MIR array.new_default element storage is not defaultable",
        ));
    }
    if require_value(definitions, *length, *span)? != ValueType::I32 {
        return Err(mir_error(*span, "MIR array.new_default length must be i32"));
    }
    let ValueType::Ref(source_reference) = require_value(definitions, *source, *span)? else {
        return Err(mir_error(
            *span,
            "MIR array conversion source must be an array",
        ));
    };
    let HeapType::Index(source_index) = source_reference.heap else {
        return Err(mir_error(
            *span,
            "MIR array conversion source must have a concrete layout",
        ));
    };
    if !matches!(
        composite_at(defined, source_index),
        Some(CompositeType::Array(_))
    ) {
        return Err(mir_error(
            *span,
            "MIR array conversion source type is not an array",
        ));
    }
    if !is_array_reference(
        value_type(function, *destination)
            .ok_or_else(|| mir_error(*span, "MIR array.new_default result has no value type"))?,
        *type_index,
        defined,
    ) {
        return Err(mir_error(
            *span,
            "MIR array.new_default result must match its array type",
        ));
    }
    if require_value(definitions, *index, *span)? != ValueType::I32 {
        return Err(mir_error(*span, "MIR array conversion index must be i32"));
    }
    Ok(())
}

pub(super) fn verify_clone(
    function: &Function,
    destination: ValueId,
    type_index: DefinedTypeId,
    value: ValueId,
    span: psrs_span::TextRange,
    definitions: &HashMap<ValueId, ValueType>,
    defined: &[&crate::types::DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Some(CompositeType::Array(element)) = composite_at(defined, type_index) else {
        return Err(mir_error(span, "MIR array.clone type is not an array"));
    };
    // A clone lowers to `array.new_default` + `array.copy`, so the array must be
    // mutable and its element storage must have a zero value.
    if !element.mutable {
        return Err(mir_error(span, "MIR array.clone requires a mutable array"));
    }
    if !is_defaultable_storage(&element.storage) {
        return Err(mir_error(
            span,
            "MIR array.clone element storage is not defaultable",
        ));
    }
    if !is_array_reference(
        require_value(definitions, value, span)?,
        type_index,
        defined,
    ) {
        return Err(mir_error(
            span,
            "MIR array.clone operand must be a reference",
        ));
    }
    if !is_array_reference(
        value_type(function, destination)
            .ok_or_else(|| mir_error(span, "MIR array.clone result has no value type"))?,
        type_index,
        defined,
    ) {
        return Err(mir_error(
            span,
            "MIR array.clone result must be a reference",
        ));
    }
    Ok(())
}

/// `array.new_default` needs a zero value: numeric and vector storage default
/// to zero and nullable references default to null. A non-null reference has no
/// default, so it is rejected.
fn is_defaultable_storage(storage: &StorageType) -> bool {
    match storage {
        StorageType::Ref(reference) => reference.nullable,
        StorageType::I8
        | StorageType::I16
        | StorageType::I32
        | StorageType::I64
        | StorageType::F32
        | StorageType::F64
        | StorageType::V128 => true,
    }
}

pub(super) fn verify_len(
    function: &Function,
    destination: ValueId,
    value: ValueId,
    span: psrs_span::TextRange,
    definitions: &HashMap<ValueId, ValueType>,
    defined: &[&crate::types::DefinedType],
) -> Result<(), Vec<BackendError>> {
    if !is_any_array_reference(require_value(definitions, value, span)?, defined) {
        return Err(mir_error(span, "MIR array.len operand must be a reference"));
    }
    if value_type(function, destination) != Some(ValueType::I32) {
        return Err(mir_error(span, "MIR array.len result must be i32"));
    }
    Ok(())
}
