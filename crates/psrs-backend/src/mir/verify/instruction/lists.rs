//! Checks for the canonical non-byte list copy.

use super::super::util::{composite_at, is_array_reference, mir_error, require_value};
use crate::BackendError;
use crate::abi::canonical::CanonicalType;
use crate::cc::{GuestLayout, RefShape, ValueShape};
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
        struct_type,
        pointer,
        length,
        element,
        element_guest,
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
    if *direction == ListDirection::Free {
        return Ok(());
    }
    let Some(CompositeType::Array(field)) = composite_at(defined, *array_type) else {
        return Err(mir_error(*span, "MIR list copy type is not an array"));
    };
    let erased_scalar = matches!(element_guest,
        GuestLayout::Scalar { shape: ValueShape::Reference(reference) }
        if reference.heap == RefShape::Erased)
        && matches!(&field.storage, StorageType::Ref(reference) if reference.heap == crate::types::HeapType::Eq)
        && matches!(
            element,
            CanonicalType::Bool
                | CanonicalType::Int { .. }
                | CanonicalType::Float { .. }
                | CanonicalType::Char
                | CanonicalType::Enum(_)
                | CanonicalType::Handle { .. }
        );
    if !erased_scalar && !element_storage_matches(element, &field.storage) {
        return Err(mir_error(
            *span,
            "MIR list copy element does not match the GC array",
        ));
    }
    if let GuestLayout::Product { labels, fields, .. } = element_guest {
        let Some(CompositeType::Struct(field_types)) = composite_at(defined, *struct_type) else {
            return Err(mir_error(*span, "MIR list copy element is not a struct"));
        };
        if field_types.len() != fields.len() || labels.len() != fields.len() {
            return Err(mir_error(
                *span,
                "MIR list copy element fields do not match the struct",
            ));
        }
        for (field, storage) in fields.iter().zip(field_types) {
            if !field_storage_matches(field.stored, &storage.storage) {
                return Err(mir_error(
                    *span,
                    "MIR list copy element field storage does not match the struct",
                ));
            }
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
        ListDirection::Free => true,
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

/// Whether the GC array element storage can hold the canonical element.
fn element_storage_matches(element: &CanonicalType, storage: &StorageType) -> bool {
    match element {
        CanonicalType::String | CanonicalType::List(_) | CanonicalType::FixedList { .. } => {
            matches!(storage, StorageType::Ref(_))
        }
        CanonicalType::Float { .. } => *storage == StorageType::F64,
        CanonicalType::Record(_)
        | CanonicalType::Flags(_)
        | CanonicalType::Option(_)
        | CanonicalType::Result { .. }
        | CanonicalType::Variant(_) => matches!(storage, StorageType::Ref(_)),
        _ => *storage == StorageType::I32,
    }
}

/// Whether the struct field storage can hold a product field shape.
fn field_storage_matches(shape: ValueShape, storage: &StorageType) -> bool {
    match shape {
        ValueShape::State => false,
        ValueShape::Integer | ValueShape::Boolean => *storage == StorageType::I32,
        ValueShape::Number => *storage == StorageType::F64,
        ValueShape::String | ValueShape::Reference(_) => matches!(storage, StorageType::Ref(_)),
    }
}
