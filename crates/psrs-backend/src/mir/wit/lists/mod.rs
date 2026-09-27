//! Copy a source array to or from a non-byte canonical `list<T>`.
//!
//! One parameterized instruction drives the copy: the canonical element and its
//! guest layout select the scalar width, the string codec, or the record/flags
//! struct projection. The same instruction frees a parameter buffer's element
//! payloads after the call.

use super::super::BlockId;
use super::super::instruction::{Instruction, ListDirection};
use super::{ElementFree, PendingFree, WitCallLowerer, free::free_plan, free_buffer};
use crate::BackendError;
use crate::abi::canonical::{CanonicalType, size_align};
use crate::abi::{self, WasiImport};
use crate::cc::{GuestLayout, ValueShape};
use crate::mir::NumericOp;
use crate::types::{DefinedTypeId, MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

/// Copies a source array into the inline canonical slots of a fixed-length
/// list parameter. A fixed-length list has no `(pointer, length)` buffer: each
/// element is lowered directly into the flattened arguments.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_fixed_list<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    element: &CanonicalType,
    length: u32,
    guest: Option<&GuestLayout>,
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let element_field = match guest {
        Some(GuestLayout::Array { element, .. }) => element,
        _ => return Err(unsupported_list(span)),
    };
    let array_type = lowerer.wit_array_type(argument, span)?;
    let mut current = current;
    for index in 0..length {
        let value = lowerer.wit_array_get(
            current,
            argument,
            array_type,
            element_field.stored,
            index,
            span,
        )?;
        current = super::parameters::lower_parameter(
            lowerer,
            value,
            Some(&element_field.value),
            element,
            flat,
            frees,
            current,
            span,
        )?;
    }
    Ok(())
}

/// Reads the inline elements of a fixed-length list result into a fresh GC
/// array. The elements sit directly in the return area, so the list copy reads
/// from `retptr` with the static element count.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_fixed_list_result<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    element: &CanonicalType,
    length: u32,
    list: Option<&GuestLayout>,
    destination: ValueId,
    arguments: Vec<ValueId>,
    retptr: Option<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let address = retptr.ok_or_else(|| {
        vec![BackendError::invalid_ir(
            "P9 MIR lowering",
            span,
            "a fixed-length list result takes a return pointer",
        )]
    })?;
    lowerer.append_wit_instruction(
        current,
        Instruction::CallVoid {
            function: import.symbol,
            arguments,
            span,
        },
        span,
    )?;
    let element_guest = element_guest(lowerer, list, element, span)?;
    let struct_type = element_struct_type(lowerer, &element_guest, span)?;
    let array_type = lowerer.wit_array_type(destination, span)?;
    let count = constant_i32(lowerer, length as i32, current, span)?;
    lowerer.append_wit_instruction(
        current,
        Instruction::ListCopy {
            direction: ListDirection::Load,
            array: destination,
            array_type,
            struct_type,
            pointer: address,
            length: count,
            element: element.clone(),
            element_guest,
            span,
        },
        span,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn write_value_list<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    element: &CanonicalType,
    guest: Option<&GuestLayout>,
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let layout = size_align(element);
    let element_guest = element_guest(lowerer, guest, element, span)?;
    let struct_type = element_struct_type(lowerer, &element_guest, span)?;
    let array_type = lowerer.wit_array_type(argument, span)?;
    let length = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::ArrayLen {
            destination: length,
            value: argument,
            span,
        },
        span,
    )?;
    let bytes = scale(lowerer, length, layout.size as i32, current, span)?;
    let pointer = allocate(lowerer, bytes, layout.align as i32, current, span)?;
    lowerer.append_wit_instruction(
        current,
        Instruction::ListCopy {
            direction: ListDirection::Store,
            array: argument,
            array_type,
            struct_type,
            pointer,
            length,
            element: element.clone(),
            element_guest: element_guest.clone(),
            span,
        },
        span,
    )?;
    flat.push(pointer);
    flat.push(length);
    let elements = element_free(element, length, element_guest);
    frees.push(PendingFree {
        pointer,
        length: bytes,
        align: layout.align as i32,
        elements,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_value_list_result<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    element: &CanonicalType,
    list: Option<&GuestLayout>,
    destination: ValueId,
    arguments: Vec<ValueId>,
    retptr: Option<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if let CanonicalType::Handle {
        ownership: crate::abi::canonical::Ownership::Borrow,
        ..
    } = element
    {
        // A borrowed handle must not escape the call that produced it. An owned
        // handle list is an ordinary `i32` array: the standard library drops
        // each extracted handle (DEC-14), so the compiler copies the indices.
        return Err(vec![BackendError::new(
            "P9 MIR lowering",
            span,
            "a list<borrow<T>> result cannot outlive the call that produced it; return list<own<T>> instead",
        )]);
    }
    let address = retptr.ok_or_else(|| {
        vec![BackendError::invalid_ir(
            "P9 MIR lowering",
            span,
            "a value list result takes a return pointer",
        )]
    })?;
    lowerer.append_wit_instruction(
        current,
        Instruction::CallVoid {
            function: import.symbol,
            arguments,
            span,
        },
        span,
    )?;
    let element_guest = element_guest(lowerer, list, element, span)?;
    let struct_type = element_struct_type(lowerer, &element_guest, span)?;
    let pointer = load(lowerer, address, 0, current, span)?;
    let length = load(lowerer, address, 4, current, span)?;
    let array_type = lowerer.wit_array_type(destination, span)?;
    lowerer.append_wit_instruction(
        current,
        Instruction::ListCopy {
            direction: ListDirection::Load,
            array: destination,
            array_type,
            struct_type,
            pointer,
            length,
            element: element.clone(),
            element_guest,
            span,
        },
        span,
    )?;
    let layout = size_align(element);
    let bytes = scale(lowerer, length, layout.size as i32, current, span)?;
    free_buffer(lowerer, pointer, bytes, layout.align as i32, current, span)
}

/// Emits the element-payload frees of a call-local list buffer.
pub(super) fn free_elements<L: WitCallLowerer>(
    lowerer: &mut L,
    pointer: ValueId,
    elements: &ElementFree,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    lowerer.append_wit_instruction(
        current,
        Instruction::ListCopy {
            direction: ListDirection::Free,
            array: pointer,
            array_type: DefinedTypeId(0),
            struct_type: DefinedTypeId(0),
            pointer,
            length: elements.count,
            element: elements.element.clone(),
            element_guest: elements.element_guest.clone(),
            span,
        },
        span,
    )
}

/// The guest layout of a list's element. A record or flags element requires its
/// product layout; a scalar or byte-list element is copied from the canonical
/// type alone, so an unresolved layout falls back to a scalar placeholder.
fn element_guest<L: WitCallLowerer>(
    lowerer: &L,
    list: Option<&GuestLayout>,
    element: &CanonicalType,
    span: TextRange,
) -> Result<GuestLayout, Vec<BackendError>> {
    if let Some(GuestLayout::Array { element: field, .. }) = list
        && let Some(layout) = lowerer.wit_guest_layout(field.stored)
    {
        return Ok(layout);
    }
    if matches!(
        element,
        CanonicalType::Record(_)
            | CanonicalType::Flags(_)
            | CanonicalType::Option(_)
            | CanonicalType::Result { .. }
            | CanonicalType::Variant(_)
            | CanonicalType::List(_)
            | CanonicalType::FixedList { .. }
    ) {
        return Err(unsupported_list(span));
    }
    Ok(GuestLayout::Scalar {
        shape: ValueShape::Integer,
    })
}

/// The concrete element struct type for a record or flags element, or a
/// placeholder for scalars and strings.
pub(super) fn element_struct_type<L: WitCallLowerer>(
    lowerer: &L,
    element_guest: &GuestLayout,
    span: TextRange,
) -> Result<DefinedTypeId, Vec<BackendError>> {
    match element_guest {
        GuestLayout::Product { repr, .. } | GuestLayout::Variant { repr, .. } => lowerer
            .wit_repr_index(*repr)
            .ok_or_else(|| unsupported_list(span)),
        _ => Ok(DefinedTypeId(0)),
    }
}

fn element_free(
    element: &CanonicalType,
    count: ValueId,
    element_guest: GuestLayout,
) -> Option<ElementFree> {
    if free_plan(element).is_no_free() {
        None
    } else {
        Some(ElementFree {
            count,
            element: element.clone(),
            element_guest,
        })
    }
}

pub(super) fn scale<L: WitCallLowerer>(
    lowerer: &mut L,
    length: ValueId,
    size: i32,
    current: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    if size == 1 {
        return Ok(length);
    }
    let stride = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Constant {
            destination: stride,
            value: size,
            span,
        },
        span,
    )?;
    let bytes = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Primitive {
            destination: bytes,
            op: NumericOp::I32Mul,
            left: length,
            right: stride,
            span,
        },
        span,
    )?;
    Ok(bytes)
}

pub(super) fn allocate<L: WitCallLowerer>(
    lowerer: &mut L,
    size: ValueId,
    align: i32,
    current: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let mut arguments = Vec::with_capacity(4);
    for value in [0, 0, align] {
        let constant = lowerer.fresh_wit_value(ValueType::I32);
        lowerer.append_wit_instruction(
            current,
            Instruction::Constant {
                destination: constant,
                value,
                span,
            },
            span,
        )?;
        arguments.push(constant);
    }
    arguments.push(size);
    let address = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Call {
            destination: address,
            function: abi::REALLOC_SYMBOL,
            arguments,
            span,
        },
        span,
    )?;
    Ok(address)
}

pub(super) fn load<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    offset: u32,
    current: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let destination = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Load {
            destination,
            address,
            memory: MemoryId(0),
            offset,
            span,
        },
        span,
    )?;
    Ok(destination)
}

fn unsupported_list(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        "this WIT list element has no source array lowering",
    )]
}

fn constant_i32<L: WitCallLowerer>(
    lowerer: &mut L,
    value: i32,
    current: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let destination = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Constant {
            destination,
            value,
            span,
        },
        span,
    )?;
    Ok(destination)
}
