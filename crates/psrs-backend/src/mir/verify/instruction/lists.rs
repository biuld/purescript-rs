//! Checks for the canonical non-byte list copy.

use super::super::util::{composite_at, is_array_reference, mir_error, require_value};
use crate::BackendError;
use crate::abi::ListElement;
use crate::abi::layout::SlotKind;
use crate::mir::{Function, Instruction, ListDirection, ValueId, ValueType};
use crate::types::{CompositeType, DefinedType, StorageType};
use std::collections::HashMap;

pub(super) fn verify_list_copy(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::ListCopy {
        direction,
        array,
        array_type,
        pointer,
        length,
        element,
        span,
    } = instruction
    else {
        unreachable!("list verifier received another instruction")
    };
    if require_value(definitions, *pointer, *span)? != ValueType::I32
        || require_value(definitions, *length, *span)? != ValueType::I32
    {
        return Err(mir_error(
            *span,
            "MIR list copy pointer and length must be i32",
        ));
    }
    if *direction == ListDirection::FreeStrings {
        return Ok(());
    }
    let Some(CompositeType::Array(field)) = composite_at(defined, *array_type) else {
        return Err(mir_error(*span, "MIR list copy type is not an array"));
    };
    if !element_storage_matches(*element, &field.storage) {
        return Err(mir_error(
            *span,
            "MIR list copy element does not match the GC array",
        ));
    }
    let array_type_ok = match direction {
        ListDirection::Load => value_type(function, *array)
            .is_some_and(|ty| is_array_reference(ty, *array_type, defined)),
        ListDirection::Store => is_array_reference(
            require_value(definitions, *array, *span)?,
            *array_type,
            defined,
        ),
        ListDirection::FreeStrings => true,
    };
    if !array_type_ok {
        return Err(mir_error(*span, "MIR list copy array has the wrong type"));
    }
    Ok(())
}

fn value_type(function: &Function, value: ValueId) -> Option<ValueType> {
    function
        .values
        .iter()
        .find(|decl| decl.id == value)
        .map(|decl| decl.ty)
}

fn element_storage_matches(element: ListElement, storage: &StorageType) -> bool {
    match element {
        ListElement::Word
        | ListElement::Narrow { .. }
        | ListElement::Boolean
        | ListElement::Scalar64 { .. } => *storage == StorageType::I32,
        ListElement::Float32 | ListElement::Float64 => *storage == StorageType::F64,
        ListElement::String => matches!(storage, StorageType::Ref(reference) if reference.nullable),
    }
}

pub(super) fn verify_list_copy_record(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
    defined: &[&DefinedType],
) -> Result<(), Vec<BackendError>> {
    let Instruction::ListCopyRecord {
        direction,
        array,
        array_type,
        struct_type,
        pointer,
        length,
        fields,
        span,
        ..
    } = instruction
    else {
        unreachable!("record list verifier received another instruction")
    };
    if require_value(definitions, *pointer, *span)? != ValueType::I32
        || require_value(definitions, *length, *span)? != ValueType::I32
    {
        return Err(mir_error(
            *span,
            "MIR record list copy pointer and length must be i32",
        ));
    }
    if *direction == ListDirection::FreeStrings {
        return Ok(());
    }
    let Some(CompositeType::Array(_)) = composite_at(defined, *array_type) else {
        return Err(mir_error(
            *span,
            "MIR record list copy type is not an array",
        ));
    };
    let Some(CompositeType::Struct(field_types)) = composite_at(defined, *struct_type) else {
        return Err(mir_error(
            *span,
            "MIR record list copy element is not a struct",
        ));
    };
    if field_types.len() != fields.len() {
        return Err(mir_error(
            *span,
            "MIR record list copy field count does not match the struct",
        ));
    }
    for (field, storage) in fields.iter().zip(field_types) {
        let expected = match field.kind {
            SlotKind::Byte | SlotKind::Half | SlotKind::Word => StorageType::I32,
            SlotKind::F64 => StorageType::F64,
            SlotKind::I64 | SlotKind::F32 => {
                return Err(mir_error(
                    *span,
                    "MIR record list copy field width is not supported yet",
                ));
            }
        };
        if storage.storage != expected {
            return Err(mir_error(
                *span,
                "MIR record list copy field storage does not match the struct",
            ));
        }
    }
    let array_type_ok = match direction {
        ListDirection::Load => value_type(function, *array)
            .is_some_and(|ty| is_array_reference(ty, *array_type, defined)),
        ListDirection::Store => is_array_reference(
            require_value(definitions, *array, *span)?,
            *array_type,
            defined,
        ),
        ListDirection::FreeStrings => true,
    };
    if !array_type_ok {
        return Err(mir_error(
            *span,
            "MIR record list copy array has the wrong type",
        ));
    }
    Ok(())
}
