use super::super::{PendingFree, StringFree, WitCallLowerer, free_buffer};
use super::{allocate, load, scale};
use crate::BackendError;
use crate::abi::layout::SlotKind;
use crate::abi::{self, WasiImport};
use crate::cc::{RefShape, Reference, ValueShape};
use crate::mir::{BlockId, Instruction, ListDirection, ListFieldCopy};
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;

#[allow(clippy::too_many_arguments)]
pub(super) fn write_record_value_list<L: WitCallLowerer>(
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
            fields: fields.clone(),
            span,
        },
        span,
    )?;
    flat.push(pointer);
    flat.push(length);
    let has_strings = fields
        .iter()
        .any(|field| matches!(field, ListFieldCopy::String { .. }));
    frees.push(PendingFree {
        pointer,
        length: bytes,
        align: align as i32,
        string_elements: has_strings.then_some(StringFree::Records {
            count: length,
            size,
            fields,
        }),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_record_value_list_result<L: WitCallLowerer>(
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

/// Builds the record element plan: the element struct type, its fields in WIT
/// order with canonical offsets, and the element size and alignment.
fn record_plan<L: WitCallLowerer>(
    lowerer: &L,
    shape: &ValueShape,
    element: &abi::WasiParamKind,
    span: TextRange,
) -> Result<(crate::types::DefinedTypeId, Vec<ListFieldCopy>, u32, u32), Vec<BackendError>> {
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
    if product.len() != labels.len() {
        return Err(unsupported_list(span));
    }
    let index_of = |name: &str| -> Option<u32> {
        let label = abi::source_field_name(name);
        labels
            .iter()
            .position(|candidate| candidate == &label)
            .map(|index| index as u32)
    };
    let mut plan = Vec::with_capacity(fields.len());
    let mut cursor = 0;
    for field in fields {
        let index = index_of(&field.name).ok_or_else(|| unsupported_list(span))?;
        let field_shape = product
            .get(index as usize)
            .ok_or_else(|| unsupported_list(span))?;
        match &field.kind {
            kind @ (abi::WasiParamKind::Integer32
            | abi::WasiParamKind::IntegerNarrow { .. }
            | abi::WasiParamKind::Boolean
            | abi::WasiParamKind::Char
            | abi::WasiParamKind::Enum { .. })
                if matches!(field_shape, ValueShape::Integer) =>
            {
                let slot = *layout
                    .slots
                    .get(cursor)
                    .ok_or_else(|| unsupported_list(span))?;
                plan.push(ListFieldCopy::Scalar {
                    offset: slot.offset,
                    index,
                    kind: slot.kind,
                });
                cursor += 1;
                let _ = kind;
            }
            abi::WasiParamKind::Float64 if matches!(field_shape, ValueShape::Number) => {
                let slot = *layout
                    .slots
                    .get(cursor)
                    .ok_or_else(|| unsupported_list(span))?;
                if slot.kind != SlotKind::F64 {
                    return Err(unsupported_list(span));
                }
                plan.push(ListFieldCopy::Scalar {
                    offset: slot.offset,
                    index,
                    kind: slot.kind,
                });
                cursor += 1;
            }
            abi::WasiParamKind::List if matches!(field_shape, ValueShape::String) => {
                let pointer = *layout
                    .slots
                    .get(cursor)
                    .ok_or_else(|| unsupported_list(span))?;
                let length = *layout
                    .slots
                    .get(cursor + 1)
                    .ok_or_else(|| unsupported_list(span))?;
                if pointer.kind != SlotKind::Word
                    || length.kind != SlotKind::Word
                    || pointer.offset + 4 != length.offset
                {
                    return Err(unsupported_list(span));
                }
                plan.push(ListFieldCopy::String {
                    offset: pointer.offset,
                    index,
                });
                cursor += 2;
            }
            _ => return Err(unsupported_list(span)),
        }
    }
    if cursor != layout.slots.len() {
        return Err(unsupported_list(span));
    }
    Ok((struct_type, plan, layout.size, layout.align))
}

pub(super) fn reference_repr(shape: &ValueShape) -> Option<crate::cc::ReprId> {
    let ValueShape::Reference(Reference {
        heap: RefShape::Repr(repr),
        ..
    }) = shape
    else {
        return None;
    };
    Some(*repr)
}

pub(super) fn unsupported_list(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        "this WIT list element has no source array lowering",
    )]
}

/// Frees the string buffers a `list<record>` parameter allocated element-wise.
pub(crate) fn free_record_string_elements<L: WitCallLowerer>(
    lowerer: &mut L,
    pointer: ValueId,
    count: ValueId,
    size: u32,
    fields: &[ListFieldCopy],
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    lowerer.append_wit_instruction(
        current,
        Instruction::ListCopyRecord {
            direction: ListDirection::FreeStrings,
            array: pointer,
            array_type: crate::types::DefinedTypeId(0),
            struct_type: crate::types::DefinedTypeId(0),
            pointer,
            length: count,
            size,
            fields: fields.to_vec(),
            span,
        },
        span,
    )
}
