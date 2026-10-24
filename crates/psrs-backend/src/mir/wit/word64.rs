//! Bit-preserving conversion between one canonical i64 and two source words.

use super::{BlockId, Instruction, WitCallLowerer};
use crate::BackendError;
use crate::cc::GuestLayout;
use crate::types::{HeapType, RefType, ValueId, ValueType};
use psrs_span::TextRange;

fn invalid(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P9 canonical ABI",
        span,
        "64-bit word carrier has no checked product layout",
    )]
}

pub(super) fn pack<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let high = lowerer.wit_product_field(block, value, 0, span)?;
    let low = lowerer.wit_product_field(block, value, 1, span)?;
    let destination = lowerer.fresh_wit_value(ValueType::I64);
    lowerer.append_wit_instruction(
        block,
        Instruction::JoinI64 {
            destination,
            high,
            low,
            span,
        },
        span,
    )?;
    Ok(destination)
}

pub(super) fn unpack<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    guest: &GuestLayout,
    block: BlockId,
    span: TextRange,
) -> Result<(ValueId, BlockId), Vec<BackendError>> {
    if !crate::abi::word64::is_guest(guest) {
        return Err(invalid(span));
    }
    if let GuestLayout::RecordStorage {
        product,
        labels,
        fields,
        ..
    } = guest
    {
        let logical = GuestLayout::Product {
            repr: *product,
            labels: labels.clone(),
            fields: fields.clone(),
        };
        let (value, block) = unpack(lowerer, value, &logical, block, span)?;
        let (block, value) =
            lowerer.wit_payload_conversion(block, value, logical.shape(), true, span)?;
        let value = super::aggregate::cast_reference(lowerer, value, &guest.shape(), block, span)?;
        return Ok((value, block));
    }
    let GuestLayout::Product { repr, .. } = guest else {
        return Err(invalid(span));
    };
    let high = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        block,
        Instruction::HighI64 {
            destination: high,
            value,
            span,
        },
        span,
    )?;
    let low = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        block,
        Instruction::WrapI64 {
            destination: low,
            value,
            span,
        },
        span,
    )?;
    let type_index = lowerer.wit_repr_index(*repr).ok_or_else(|| invalid(span))?;
    let destination = lowerer.fresh_wit_value(ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(type_index),
    }));
    lowerer.append_wit_instruction(
        block,
        Instruction::StructNew {
            destination,
            type_index,
            arguments: vec![high, low],
            span,
        },
        span,
    )?;
    Ok((destination, block))
}
