//! Copy a source array to or from a non-byte canonical `list<T>`.

use super::super::BlockId;
use super::super::instruction::{Instruction, ListDirection};
use super::{PendingFree, WitCallLowerer, free_buffer};
use crate::BackendError;
use crate::abi::layout::SlotKind;
use crate::abi::{self, WasiImport};
use crate::cc::{RefShape, Reference, ValueShape};
use crate::mir::{ListRecordField, NumericOp};
use crate::types::{MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

#[allow(clippy::too_many_arguments)]
pub(super) fn write_value_list<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    shape: &ValueShape,
    element: &abi::WasiParamKind,
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if matches!(element, abi::WasiParamKind::Record { .. }) {
        return write_record_value_list(
            lowerer, argument, shape, element, flat, frees, current, span,
        );
    }
    let element = list_element(element, span)?;
    let (size, align) = abi::element_layout(element);
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
    let bytes = scale(lowerer, length, size, current, span)?;
    let pointer = allocate(lowerer, bytes, align, current, span)?;
    lowerer.append_wit_instruction(
        current,
        Instruction::ListCopy {
            direction: ListDirection::Store,
            array: argument,
            array_type,
            pointer,
            length,
            element,
            span,
        },
        span,
    )?;
    flat.push(pointer);
    flat.push(length);
    frees.push(PendingFree {
        pointer,
        length: bytes,
        align,
        string_elements: matches!(element, abi::ListElement::String).then_some(length),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_record_value_list<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    shape: &ValueShape,
    element: &abi::WasiParamKind,
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let (struct_type, fields, size, align) = record_plan(lowerer, shape, element, span)?;
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
    let bytes = scale(lowerer, length, size as i32, current, span)?;
    let pointer = allocate(lowerer, bytes, align as i32, current, span)?;
    lowerer.append_wit_instruction(
        current,
        Instruction::ListCopyRecord {
            direction: ListDirection::Store,
            array: argument,
            array_type,
            struct_type,
            pointer,
            length,
            size,
            fields,
            span,
        },
        span,
    )?;
    flat.push(pointer);
    flat.push(length);
    frees.push(PendingFree {
        pointer,
        length: bytes,
        align: align as i32,
        string_elements: None,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_value_list_result<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    element: &abi::WasiParamKind,
    shape: &ValueShape,
    destination: ValueId,
    arguments: Vec<ValueId>,
    retptr: Option<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if matches!(element, abi::WasiParamKind::Record { .. }) {
        return read_record_value_list_result(
            lowerer,
            import,
            element,
            shape,
            destination,
            arguments,
            retptr,
            current,
            span,
        );
    }
    let element = list_element(element, span)?;
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
    let pointer = load(lowerer, address, 0, current, span)?;
    let length = load(lowerer, address, 4, current, span)?;
    let array_type = lowerer.wit_array_type(destination, span)?;
    lowerer.append_wit_instruction(
        current,
        Instruction::ListCopy {
            direction: ListDirection::Load,
            array: destination,
            array_type,
            pointer,
            length,
            element,
            span,
        },
        span,
    )?;
    let (size, align) = abi::element_layout(element);
    let bytes = scale(lowerer, length, size, current, span)?;
    free_buffer(lowerer, pointer, bytes, align, current, span)
}

#[allow(clippy::too_many_arguments)]
fn read_record_value_list_result<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    element: &abi::WasiParamKind,
    shape: &ValueShape,
    destination: ValueId,
    arguments: Vec<ValueId>,
    retptr: Option<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let (struct_type, fields, size, align) = record_plan(lowerer, shape, element, span)?;
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
    let pointer = load(lowerer, address, 0, current, span)?;
    let length = load(lowerer, address, 4, current, span)?;
    let array_type = lowerer.wit_array_type(destination, span)?;
    lowerer.append_wit_instruction(
        current,
        Instruction::ListCopyRecord {
            direction: ListDirection::Load,
            array: destination,
            array_type,
            struct_type,
            pointer,
            length,
            size,
            fields,
            span,
        },
        span,
    )?;
    let bytes = scale(lowerer, length, size as i32, current, span)?;
    free_buffer(lowerer, pointer, bytes, align as i32, current, span)
}

/// Builds the record element plan: the element struct type, its scalar fields
/// in struct order with canonical offsets, and the element size and alignment.
fn record_plan<L: WitCallLowerer>(
    lowerer: &L,
    shape: &ValueShape,
    element: &abi::WasiParamKind,
    span: TextRange,
) -> Result<(crate::types::DefinedTypeId, Vec<ListRecordField>, u32, u32), Vec<BackendError>> {
    let array_repr = reference_repr(shape).ok_or_else(|| unsupported_list(span))?;
    let element_shape = lowerer
        .wit_array_element(array_repr)
        .ok_or_else(|| unsupported_list(span))?;
    let element_repr = reference_repr(&element_shape).ok_or_else(|| unsupported_list(span))?;
    let (product, labels) = lowerer
        .wit_product(element_repr)
        .ok_or_else(|| unsupported_list(span))?;
    let struct_type = lowerer
        .wit_repr_index(element_repr)
        .ok_or_else(|| unsupported_list(span))?;
    let layout = abi::layout::parameter_layout(element).ok_or_else(|| unsupported_list(span))?;
    let abi::WasiParamKind::Record { fields } = element else {
        return Err(unsupported_list(span));
    };
    if layout.slots.len() != fields.len() || product.len() != labels.len() {
        return Err(unsupported_list(span));
    }
    let mut plan = Vec::with_capacity(product.len());
    for (index, (label, field_shape)) in labels.iter().zip(&product).enumerate() {
        let wit_index = fields
            .iter()
            .position(|field| abi::source_field_name(&field.name) == *label)
            .ok_or_else(|| unsupported_list(span))?;
        let slot = layout.slots[wit_index];
        match (slot.kind, field_shape) {
            (SlotKind::Byte | SlotKind::Half | SlotKind::Word, ValueShape::Integer)
            | (SlotKind::F64, ValueShape::Number) => {}
            _ => return Err(unsupported_list(span)),
        }
        plan.push(ListRecordField {
            offset: slot.offset,
            index: index as u32,
            kind: slot.kind,
        });
    }
    Ok((struct_type, plan, layout.size, layout.align))
}

fn reference_repr(shape: &ValueShape) -> Option<crate::cc::ReprId> {
    let ValueShape::Reference(Reference {
        heap: RefShape::Repr(repr),
        ..
    }) = shape
    else {
        return None;
    };
    Some(*repr)
}

fn unsupported_list(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        "this WIT list element has no source array lowering",
    )]
}

pub(super) fn free_string_elements<L: WitCallLowerer>(
    lowerer: &mut L,
    pointer: ValueId,
    length: ValueId,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    lowerer.append_wit_instruction(
        current,
        Instruction::ListCopy {
            direction: ListDirection::FreeStrings,
            array: pointer,
            array_type: crate::types::DefinedTypeId(0),
            pointer,
            length,
            element: abi::ListElement::String,
            span,
        },
        span,
    )
}

fn list_element(
    kind: &abi::WasiParamKind,
    span: TextRange,
) -> Result<abi::ListElement, Vec<BackendError>> {
    abi::list_element(kind).ok_or_else(|| {
        vec![BackendError::new(
            "P9 MIR lowering",
            span,
            "this WIT list element has no source array lowering",
        )]
    })
}

fn scale<L: WitCallLowerer>(
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

fn allocate<L: WitCallLowerer>(
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

fn load<L: WitCallLowerer>(
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
