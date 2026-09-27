//! Decode for flags and non-byte list aggregate payloads.

use super::memory::load;
use super::*;

/// Builds a GC flags record from the packed canonical words.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_flags<L: WitCallLowerer>(
    lowerer: &mut L,
    representation: crate::cc::ReprId,
    names: &[String],
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let (product, labels) = lowerer
        .wit_product(representation)
        .ok_or_else(|| unsupported(span))?;
    if product.len() != labels.len() || names.len() != labels.len() {
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
    let mut values = vec![None; product.len()];
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
    _element_node: &crate::cc::PayloadNode,
    element: &WasiParamKind,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let array_type = lowerer
        .wit_repr_index(representation)
        .ok_or_else(|| unsupported(span))?;
    let destination = lowerer.fresh_wit_value(ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(array_type),
    }));
    let shape = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(representation),
    });
    if let Some(element) = abi::list_element(element) {
        let pointer = load(lowerer, address, offset, block, span)?;
        let length = load(lowerer, address, offset + 4, block, span)?;
        lowerer.append_wit_instruction(
            block,
            Instruction::ListCopy {
                direction: crate::mir::ListDirection::Load,
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
        let bytes = crate::mir::wit::lists::scale(lowerer, length, size, block, span)?;
        free_buffer(lowerer, pointer, bytes, align, block, span)?;
        return Ok(destination);
    }
    match element {
        WasiParamKind::Record { .. } => crate::mir::wit::lists::read_record_value_list_from(
            lowerer,
            element,
            &shape,
            destination,
            address,
            offset,
            block,
            span,
        )?,
        WasiParamKind::Flags { .. } => crate::mir::wit::lists::read_flags_value_list_from(
            lowerer,
            element,
            &shape,
            destination,
            address,
            offset,
            block,
            span,
        )?,
        _ => return Err(unsupported(span)),
    }
    Ok(destination)
}
