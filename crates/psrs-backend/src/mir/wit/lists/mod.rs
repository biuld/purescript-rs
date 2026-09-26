//! Copy a source array to or from a non-byte canonical `list<T>`.

mod record;

pub(super) use record::free_record_string_elements;
use record::{read_record_value_list_result, write_record_value_list};

use super::super::BlockId;
use super::super::instruction::{Instruction, ListDirection};
use super::{PendingFree, StringFree, WitCallLowerer, free_buffer};
use crate::BackendError;
use crate::abi::{self, WasiImport};
use crate::cc::ValueShape;
use crate::mir::NumericOp;
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
        string_elements: matches!(element, abi::ListElement::String)
            .then_some(StringFree::Scalars(length)),
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
