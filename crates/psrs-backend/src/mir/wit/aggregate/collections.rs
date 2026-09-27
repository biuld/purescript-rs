//! Decode for flags and non-byte list aggregate payloads.

use super::memory::load;
use super::*;
use crate::mir::wit::free_buffer;
use crate::mir::wit::lists::scale;

/// Builds a GC flags record from the packed canonical words.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_flags<L: WitCallLowerer>(
    lowerer: &mut L,
    representation: crate::cc::ReprId,
    labels: &[String],
    names: &[String],
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    if names.len() != labels.len() {
        return Err(unsupported(span));
    }
    let words = names.len().div_ceil(32);
    let mut word_values = Vec::with_capacity(words);
    for index in 0..words {
        word_values.push(load(
            lowerer,
            address,
            offset + 4 * index as u32,
            block,
            span,
        )?);
    }
    let mut values = vec![None; labels.len()];
    for (bit, name) in names.iter().enumerate() {
        let label = abi::source_field_name(name);
        let index = labels
            .iter()
            .position(|candidate| candidate == &label)
            .ok_or_else(|| unsupported(span))?;
        let shifted = if bit % 32 == 0 {
            word_values[bit / 32]
        } else {
            let shift = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                block,
                Instruction::Constant {
                    destination: shift,
                    value: (bit % 32) as i32,
                    span,
                },
                span,
            )?;
            let shifted = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                block,
                Instruction::Primitive {
                    destination: shifted,
                    op: crate::mir::NumericOp::I32ShrU,
                    left: word_values[bit / 32],
                    right: shift,
                    span,
                },
                span,
            )?;
            shifted
        };
        let one = lowerer.fresh_wit_value(ValueType::I32);
        lowerer.append_wit_instruction(
            block,
            Instruction::Constant {
                destination: one,
                value: 1,
                span,
            },
            span,
        )?;
        let bit_value = lowerer.fresh_wit_value(ValueType::I32);
        lowerer.append_wit_instruction(
            block,
            Instruction::Primitive {
                destination: bit_value,
                op: crate::mir::NumericOp::I32And,
                left: shifted,
                right: one,
                span,
            },
            span,
        )?;
        values[index] = Some(bit_value);
    }
    let values = values
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| unsupported(span))?;
    let type_index = lowerer
        .wit_repr_index(representation)
        .ok_or_else(|| unsupported(span))?;
    let destination = lowerer.fresh_wit_value(ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(type_index),
    }));
    lowerer.append_wit_instruction(
        block,
        Instruction::StructNew {
            destination,
            type_index,
            arguments: values,
            span,
        },
        span,
    )?;
    Ok(destination)
}

/// Rebuilds a non-byte GC array from a canonical `(pointer, length)` buffer.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_value_list<L: WitCallLowerer>(
    lowerer: &mut L,
    representation: crate::cc::ReprId,
    element_shape: ValueShape,
    element: &CanonicalType,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let array_type = lowerer
        .wit_repr_index(representation)
        .ok_or_else(|| unsupported(span))?;
    let element_guest = lowerer
        .wit_guest_layout(element_shape)
        .ok_or_else(|| unsupported(span))?;
    let struct_type = element_struct_type(lowerer, &element_guest, span)?;
    let destination = lowerer.fresh_wit_value(ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(array_type),
    }));
    let pointer = load(lowerer, address, offset, block, span)?;
    let length = load(lowerer, address, offset + 4, block, span)?;
    lowerer.append_wit_instruction(
        block,
        Instruction::ListCopy {
            direction: crate::mir::ListDirection::Load,
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
    let layout = crate::abi::canonical::size_align(element);
    let bytes = scale(lowerer, length, layout.size as i32, block, span)?;
    free_buffer(lowerer, pointer, bytes, layout.align as i32, block, span)?;
    Ok(destination)
}

/// Rebuilds a non-byte GC array from a fixed-length list whose elements are
/// inline at `offset` in the return area.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_fixed_list<L: WitCallLowerer>(
    lowerer: &mut L,
    representation: crate::cc::ReprId,
    element_shape: ValueShape,
    element: &CanonicalType,
    length: u32,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let array_type = lowerer
        .wit_repr_index(representation)
        .ok_or_else(|| unsupported(span))?;
    let element_guest = lowerer
        .wit_guest_layout(element_shape)
        .ok_or_else(|| unsupported(span))?;
    let struct_type = element_struct_type(lowerer, &element_guest, span)?;
    let destination = lowerer.fresh_wit_value(ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(array_type),
    }));
    let pointer = if offset == 0 {
        address
    } else {
        let constant = lowerer.fresh_wit_value(ValueType::I32);
        lowerer.append_wit_instruction(
            block,
            Instruction::Constant {
                destination: constant,
                value: offset as i32,
                span,
            },
            span,
        )?;
        let sum = lowerer.fresh_wit_value(ValueType::I32);
        lowerer.append_wit_instruction(
            block,
            Instruction::Primitive {
                destination: sum,
                op: crate::mir::NumericOp::I32Add,
                left: address,
                right: constant,
                span,
            },
            span,
        )?;
        sum
    };
    let count = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        block,
        Instruction::Constant {
            destination: count,
            value: length as i32,
            span,
        },
        span,
    )?;
    lowerer.append_wit_instruction(
        block,
        Instruction::ListCopy {
            direction: crate::mir::ListDirection::Load,
            array: destination,
            array_type,
            struct_type,
            pointer,
            length: count,
            element: element.clone(),
            element_guest,
            span,
        },
        span,
    )?;
    Ok(destination)
}

/// The concrete element struct type for a record or flags element, or a
/// placeholder for scalars and strings.
pub(super) fn element_struct_type<L: WitCallLowerer>(
    lowerer: &L,
    element_guest: &GuestLayout,
    span: TextRange,
) -> Result<crate::types::DefinedTypeId, Vec<BackendError>> {
    match element_guest {
        GuestLayout::Product { repr, .. } => lowerer
            .wit_repr_index(*repr)
            .ok_or_else(|| unsupported(span)),
        _ => Ok(crate::types::DefinedTypeId(0)),
    }
}
