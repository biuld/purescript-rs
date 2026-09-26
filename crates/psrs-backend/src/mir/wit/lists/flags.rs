use super::super::{PendingFree, WitCallLowerer};
use super::record::{reference_repr, unsupported_list};
use super::{allocate, load, scale};
use crate::BackendError;
use crate::abi::{self, WasiImport};
use crate::cc::ValueShape;
use crate::mir::{BlockId, Instruction, ListDirection, ListFlagsField};
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;

#[allow(clippy::too_many_arguments)]
pub(super) fn write_flags_value_list<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    shape: &ValueShape,
    element: &abi::WasiParamKind,
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let (struct_type, fields, size, align) = flags_plan(lowerer, shape, element, span)?;
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
        Instruction::ListCopyFlags {
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
pub(super) fn read_flags_value_list_result<L: WitCallLowerer>(
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
    let (struct_type, fields, size, align) = flags_plan(lowerer, shape, element, span)?;
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
        Instruction::ListCopyFlags {
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
    super::free_buffer(lowerer, pointer, bytes, align as i32, current, span)
}

/// Builds the flags element plan: the element struct type and the bit position
/// of each boolean field in the single canonical word. Supports up to 32 flags.
fn flags_plan<L: WitCallLowerer>(
    lowerer: &L,
    shape: &ValueShape,
    element: &abi::WasiParamKind,
    span: TextRange,
) -> Result<(crate::types::DefinedTypeId, Vec<ListFlagsField>, u32, u32), Vec<BackendError>> {
    let abi::WasiParamKind::Flags { names } = element else {
        return Err(unsupported_list(span));
    };
    if names.len() > 32 {
        return Err(unsupported_list(span));
    }
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
    if product.len() != names.len()
        || labels.len() != names.len()
        || product.iter().any(|shape| *shape != ValueShape::Boolean)
    {
        return Err(unsupported_list(span));
    }
    let mut fields = Vec::with_capacity(labels.len());
    for (index, label) in labels.iter().enumerate() {
        let bit = names
            .iter()
            .position(|name| abi::source_field_name(name) == *label)
            .ok_or_else(|| unsupported_list(span))?;
        fields.push(ListFlagsField {
            bit: bit as u32,
            index: index as u32,
        });
    }
    Ok((struct_type, fields, 4, 4))
}
