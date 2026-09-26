use super::super::super::instruction::Instruction;
use super::super::BlockId;
use super::super::{PendingFree, WitCallLowerer};
use crate::BackendError;
use crate::abi;
use crate::abi::layout::{MemorySlot, SlotKind};
use crate::types::{MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

pub(super) fn write_parameter_record<L: WitCallLowerer>(
    lowerer: &mut L,
    kinds: &[abi::WasiParamKind],
    flattened: &[ValueId],
    output: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let layout = abi::layout::record_layout(kinds.iter().map(abi::layout::parameter_layout))
        .ok_or_else(|| unsupported_parameter(span))?;
    if layout.slots.len() != flattened.len() || layout.size == 0 {
        return Err(unsupported_parameter(span));
    }
    let address = allocate_parameter_area(lowerer, layout.size, layout.align, current, span)?;
    for (slot, value) in layout.slots.iter().zip(flattened) {
        store_slot(lowerer, address, *value, *slot, current, span)?;
    }
    // The parameter record is call-local: free it after the call returns.
    let size = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Constant {
            destination: size,
            value: layout.size as i32,
            span,
        },
        span,
    )?;
    frees.push(PendingFree {
        pointer: address,
        length: size,
        align: layout.align as i32,
        string_elements: None,
    });
    output.push(address);
    Ok(())
}

fn allocate_parameter_area<L: WitCallLowerer>(
    lowerer: &mut L,
    size: u32,
    align: u32,
    current: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let mut arguments = Vec::with_capacity(4);
    for value in [0, 0, align as i32, size as i32] {
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

pub(crate) fn store_slot<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    value: ValueId,
    slot: MemorySlot,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let instruction = match slot.kind {
        SlotKind::Byte => Instruction::Store8 {
            address,
            value,
            memory: MemoryId(0),
            offset: slot.offset,
            span,
        },
        SlotKind::Half => Instruction::Store16 {
            address,
            value,
            memory: MemoryId(0),
            offset: slot.offset,
            span,
        },
        SlotKind::Word => Instruction::Store {
            address,
            value,
            memory: MemoryId(0),
            offset: slot.offset,
            span,
        },
        SlotKind::I64 => Instruction::StoreI64 {
            address,
            value,
            memory: MemoryId(0),
            offset: slot.offset,
            span,
        },
        SlotKind::F32 => Instruction::StoreF32 {
            address,
            value,
            memory: MemoryId(0),
            offset: slot.offset,
            span,
        },
        SlotKind::F64 => Instruction::StoreF64 {
            address,
            value,
            memory: MemoryId(0),
            offset: slot.offset,
            span,
        },
    };
    lowerer.append_wit_instruction(current, instruction, span)
}

fn unsupported_parameter(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        "this WIT parameter shape is not supported by the source ABI",
    )]
}
