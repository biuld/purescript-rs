//! Shared helpers for MIR verification: value lookup, reference and storage
//! classification, and diagnostics.

use crate::BackendError;
use crate::mir::{Function, ValueId, ValueType};
use crate::types::{CompositeType, DefinedType, DefinedTypeId, HeapType, StorageType};
use psrs_span::TextRange;
use std::collections::HashMap;

pub(super) fn composite_at<'a>(
    defined: &[&'a DefinedType],
    index: DefinedTypeId,
) -> Option<&'a CompositeType> {
    defined.get(index.0 as usize).map(|def| &def.composite)
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

pub(super) fn value_type_assignable(actual: ValueType, expected: ValueType) -> bool {
    match (actual, expected) {
        (ValueType::Ref(actual), ValueType::Ref(expected)) => {
            actual.heap == expected.heap && (expected.nullable || !actual.nullable)
        }
        // A logical `Boolean` value uses Wasm's `i32` storage, exactly as in a
        // struct field, so it is assignable to and from `i32` array slots.
        (ValueType::Boolean, ValueType::I32) | (ValueType::I32, ValueType::Boolean) => true,
        _ => actual == expected,
    }
}

/// Boolean values retain their MIR type while using Wasm's `i32` struct
/// storage. This compatibility applies to product fields at the GC boundary.
pub(super) fn struct_field_type_compatible(actual: ValueType, expected: ValueType) -> bool {
    actual == expected || matches!((actual, expected), (ValueType::Boolean, ValueType::I32))
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
        && index.0 as usize >= defined.len()
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

/// Checks whether a reference can be used where a concrete struct type is
/// required. Aggregate values may use the abstract `struct` heap type at a
/// CC/MIR boundary; field operations name the concrete type index.
pub(super) fn is_struct_reference(
    ty: ValueType,
    type_index: DefinedTypeId,
    defined: &[&DefinedType],
) -> bool {
    is_reference_to_composite(ty, type_index, HeapType::Struct, defined, |composite| {
        matches!(composite, CompositeType::Struct(_))
    })
}

/// Checks whether a reference can be used where a concrete array type is
/// required.
pub(super) fn is_array_reference(
    ty: ValueType,
    type_index: DefinedTypeId,
    defined: &[&DefinedType],
) -> bool {
    is_reference_to_composite(ty, type_index, HeapType::Array, defined, |composite| {
        matches!(composite, CompositeType::Array(_))
    })
}

/// Checks whether a reference has any array heap type. This is used by
/// `array.len`, which does not carry a redundant concrete type index.
pub(super) fn is_any_array_reference(ty: ValueType, defined: &[&DefinedType]) -> bool {
    let ValueType::Ref(reference) = ty else {
        return false;
    };
    match reference.heap {
        HeapType::Array => true,
        HeapType::Index(index) => {
            matches!(composite_at(defined, index), Some(CompositeType::Array(_)))
        }
        _ => false,
    }
}

fn is_reference_to_composite(
    ty: ValueType,
    type_index: DefinedTypeId,
    abstract_heap: HeapType,
    defined: &[&DefinedType],
    predicate: impl FnOnce(&CompositeType) -> bool,
) -> bool {
    let ValueType::Ref(reference) = ty else {
        return false;
    };
    match reference.heap {
        heap if heap == abstract_heap => {
            matches!(composite_at(defined, type_index), Some(composite) if predicate(composite))
        }
        HeapType::Index(actual) if actual == type_index => {
            matches!(composite_at(defined, type_index), Some(composite) if predicate(composite))
        }
        _ => false,
    }
}

pub(super) fn mir_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P9 MIR verification",
        span,
        message,
    )]
}

/// `Boolean` is a logical MIR type but uses the same i32 representation as a
/// canonical ABI scalar.
pub(super) fn call_value_types_match(actual: ValueType, expected: ValueType) -> bool {
    actual == expected
        || matches!(
            (actual, expected),
            (ValueType::Boolean, ValueType::I32) | (ValueType::I32, ValueType::Boolean)
        )
}
