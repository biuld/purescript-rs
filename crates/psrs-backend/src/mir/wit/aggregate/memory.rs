//! Canonical memory loads for aggregate payload decode.

use super::*;
use crate::abi::layout;

pub(super) fn load_discriminant<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    offset: u32,
    cases: usize,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    match layout::discriminant_width(cases) {
        1 => load8(lowerer, address, offset, block, span),
        4 => load(lowerer, address, offset, block, span),
        _ => Err(unsupported(span)),
    }
}

pub(super) fn load<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let destination = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        block,
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

pub(super) fn load8<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let destination = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        block,
        Instruction::Load8U {
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

pub(super) fn load_i64<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let destination = lowerer.fresh_wit_value(ValueType::I64);
    lowerer.append_wit_instruction(
        block,
        Instruction::LoadI64 {
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

pub(super) fn load_f32<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let destination = lowerer.fresh_wit_value(ValueType::F32);
    lowerer.append_wit_instruction(
        block,
        Instruction::LoadF32 {
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

pub(super) fn load_f64<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let destination = lowerer.fresh_wit_value(ValueType::F64);
    lowerer.append_wit_instruction(
        block,
        Instruction::LoadF64 {
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
