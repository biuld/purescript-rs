//! Checks for the canonical non-byte list copy.

use super::super::util::{composite_at, is_array_reference, mir_error, require_value};
use crate::BackendError;
use crate::abi::ListElement;
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
