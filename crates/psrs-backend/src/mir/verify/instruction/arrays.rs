//! Array instruction checks for MIR verification.

use super::super::util::{
    composite_at, is_any_array_reference, is_array_reference, mir_error, require_value,
    storage_value_type, value_type, value_type_assignable,
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

/// `array.new_data` materializes static literal bytes into a fresh packed
/// array. Only the GC string's packed `i8` element type is admitted.
pub(super) fn verify_array_new_data(
    function: &Function,
    instruction: &Instruction,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::ArrayNewData {
        destination,
        type_index,
        span,
        ..
    } = instruction
    else {
        unreachable!("array.new_data verifier received another instruction")
    };
    let Some(CompositeType::Array(element)) = composite_at(defined, *type_index) else {
        return Err(mir_error(*span, "MIR array.new_data type is not an array"));
    };
    if element.storage != StorageType::I8 {
        return Err(mir_error(
            *span,
            "MIR array.new_data requires packed i8 element storage",
        ));
    }
    if !element.mutable {
        return Err(mir_error(
            *span,
            "MIR array.new_data requires a mutable array",
        ));
    }
    if !is_array_reference(
        value_type(function, *destination)
            .ok_or_else(|| mir_error(*span, "MIR array.new_data result has no value type"))?,
        *type_index,
        defined,
    ) {
        return Err(mir_error(
            *span,
            "MIR array.new_data result must be a reference",
        ));
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

/// One element read. `unsigned` selects the zero-extending form, which is how
/// the packed byte storage of a source string is read: a signed read would
/// return a negative `Int` for any byte above `0x7F`.
pub(super) fn verify_array_get(
    function: &Function,
    instruction: &Instruction,
    unsigned: bool,
    definitions: &HashMap<ValueId, ValueType>,
    defined: &[&crate::types::DefinedType],
) -> Result<(), Vec<BackendError>> {
    let name = if unsigned { "array.get_u" } else { "array.get" };
    let (Instruction::ArrayGet {
        destination,
        type_index,
        value,
        index,
        span,
    }
    | Instruction::ArrayGetU {
        destination,
        type_index,
        value,
        index,
        span,
    }) = instruction
    else {
        unreachable!("{name} verifier received another instruction")
    };
    let Some(CompositeType::Array(element)) = composite_at(defined, *type_index) else {
        return Err(mir_error(
            *span,
            format!("MIR {name} type is not an array").leak(),
        ));
    };
    if !is_array_reference(
        require_value(definitions, *value, *span)?,
        *type_index,
        defined,
    ) {
        return Err(mir_error(
            *span,
            format!("MIR {name} operand must be a reference").leak(),
        ));
    }
    if require_value(definitions, *index, *span)? != ValueType::I32 {
        return Err(mir_error(
            *span,
            format!("MIR {name} index must be i32").leak(),
        ));
    }
    let expected = storage_value_type(&element.storage)
        .ok_or_else(|| mir_error(*span, "MIR array element storage is not representable"))?;
    if !value_type(function, *destination)
        .is_some_and(|destination_type| value_type_assignable(expected, destination_type))
    {
        return Err(mir_error(
            *span,
            format!("MIR {name} result has the wrong type").leak(),
        ));
    }
    Ok(())
}

/// `stringToBytes` reads the GC string array and builds the `Array Int` it
/// names. Both element types are packed, and the result is a reference to the
/// target array type.
pub(super) fn verify_string_bytes(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
    defined: &[&crate::types::DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::StringToBytes {
        destination,
        type_index,
        string_type,
        value,
        span,
        ..
    } = instruction
    else {
        unreachable!("string/byte verifier received another instruction")
    };
    for array in [type_index, string_type] {
        let Some(CompositeType::Array(element)) = composite_at(defined, *array) else {
            return Err(mir_error(
                *span,
                "MIR string conversion type is not an array",
            ));
        };
        if !element.mutable {
            return Err(mir_error(
                *span,
                "MIR string conversion requires mutable array storage",
            ));
        }
        if !matches!(element.storage, StorageType::I8 | StorageType::I32) {
            return Err(mir_error(
                *span,
                "MIR string conversion requires packed i8 or i32 element storage",
            ));
        }
    }
    if !is_array_reference(
        require_value(definitions, *value, *span)?,
        *string_type,
        defined,
    ) {
        return Err(mir_error(
            *span,
            "MIR string conversion operand must be a reference",
        ));
    }
    let result = value_type(function, *destination)
        .ok_or_else(|| mir_error(*span, "MIR string conversion result has no value type"))?;
    if !is_array_reference(result, *type_index, defined) {
        return Err(mir_error(
            *span,
            "MIR string conversion result must be a reference",
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
