use super::super::super::instruction::Instruction;
use super::super::BlockId;
use super::super::{PendingFree, WitCallLowerer};
use crate::BackendError;
use crate::abi;
use crate::types::{MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

#[derive(Clone, Copy)]
struct MemorySlot {
    offset: u32,
    kind: SlotKind,
}

#[derive(Clone, Copy)]
enum SlotKind {
    Byte,
    Half,
    Word,
    I64,
    F32,
    F64,
}

struct MemoryLayout {
    size: u32,
    align: u32,
    slots: Vec<MemorySlot>,
}

pub(super) fn write_parameter_record<L: WitCallLowerer>(
    lowerer: &mut L,
    kinds: &[abi::WasiParamKind],
    flattened: &[ValueId],
    output: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let layout = record_layout(kinds.iter().map(|kind| parameter_layout(kind, span)), span)?;
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
    });
    output.push(address);
    Ok(())
}

fn parameter_layout(
    kind: &abi::WasiParamKind,
    span: TextRange,
) -> Result<MemoryLayout, Vec<BackendError>> {
    match kind {
        abi::WasiParamKind::Boolean => scalar_layout(1, SlotKind::Byte),
        abi::WasiParamKind::Integer32
        | abi::WasiParamKind::Char
        | abi::WasiParamKind::Handle(_) => scalar_layout(4, SlotKind::Word),
        abi::WasiParamKind::IntegerNarrow { bits, .. } => {
            let width = u32::from(*bits) / 8;
            scalar_layout(width, slot_for_width(width))
        }
        abi::WasiParamKind::Scalar64 { .. } => scalar_layout(8, SlotKind::I64),
        abi::WasiParamKind::Float32 => scalar_layout(4, SlotKind::F32),
        abi::WasiParamKind::Float64 => scalar_layout(8, SlotKind::F64),
        abi::WasiParamKind::Enum { cases } => {
            let width = discriminant_width(cases.len());
            scalar_layout(width, slot_for_width(width))
        }
        abi::WasiParamKind::Flags { names } => flags_layout(names.len(), span),
        abi::WasiParamKind::List => Ok(MemoryLayout {
            size: 8,
            align: 4,
            slots: vec![
                MemorySlot {
                    offset: 0,
                    kind: SlotKind::Word,
                },
                MemorySlot {
                    offset: 4,
                    kind: SlotKind::Word,
                },
            ],
        }),
        abi::WasiParamKind::Record { fields } => record_layout(
            fields
                .iter()
                .map(|field| parameter_layout(&field.kind, span)),
            span,
        ),
        abi::WasiParamKind::Unsupported => Err(unsupported_parameter(span)),
    }
}

fn record_layout(
    fields: impl IntoIterator<Item = Result<MemoryLayout, Vec<BackendError>>>,
    span: TextRange,
) -> Result<MemoryLayout, Vec<BackendError>> {
    let mut size = 0_u32;
    let mut align = 1_u32;
    let mut slots = Vec::new();
    for field in fields {
        let field = field?;
        size = align_to(size, field.align).ok_or_else(|| unsupported_parameter(span))?;
        for slot in field.slots {
            slots.push(MemorySlot {
                offset: size
                    .checked_add(slot.offset)
                    .ok_or_else(|| unsupported_parameter(span))?,
                kind: slot.kind,
            });
        }
        size = size
            .checked_add(field.size)
            .ok_or_else(|| unsupported_parameter(span))?;
        align = align.max(field.align);
    }
    size = align_to(size, align).ok_or_else(|| unsupported_parameter(span))?;
    Ok(MemoryLayout { size, align, slots })
}

fn scalar_layout(size: u32, kind: SlotKind) -> Result<MemoryLayout, Vec<BackendError>> {
    Ok(MemoryLayout {
        size,
        align: size,
        slots: vec![MemorySlot { offset: 0, kind }],
    })
}

fn flags_layout(count: usize, span: TextRange) -> Result<MemoryLayout, Vec<BackendError>> {
    let (width, words) = match count {
        0 => {
            return Ok(MemoryLayout {
                size: 0,
                align: 4,
                slots: Vec::new(),
            });
        }
        1..=8 => (1, 1),
        9..=16 => (2, 1),
        17..=32 => (4, 1),
        _ => (4, count.div_ceil(32)),
    };
    let kind = slot_for_width(width);
    let mut slots = Vec::with_capacity(words);
    for index in 0..words {
        let offset = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_mul(width))
            .ok_or_else(|| unsupported_parameter(span))?;
        slots.push(MemorySlot { offset, kind });
    }
    let size = u32::try_from(words)
        .ok()
        .and_then(|words| words.checked_mul(width))
        .ok_or_else(|| unsupported_parameter(span))?;
    Ok(MemoryLayout {
        size,
        align: width,
        slots,
    })
}

fn discriminant_width(cases: usize) -> u32 {
    const U8_MAX: usize = u8::MAX as usize;
    const U16_MAX: usize = u16::MAX as usize;
    match cases.saturating_sub(1) {
        0..=U8_MAX => 1,
        256..=U16_MAX => 2,
        _ => 4,
    }
}

fn slot_for_width(width: u32) -> SlotKind {
    match width {
        1 => SlotKind::Byte,
        2 => SlotKind::Half,
        4 => SlotKind::Word,
        _ => unreachable!("canonical ABI discriminants use 1, 2, or 4 bytes"),
    }
}

fn align_to(value: u32, align: u32) -> Option<u32> {
    value
        .checked_add(align.checked_sub(1)?)
        .map(|value| value & !(align - 1))
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

fn store_slot<L: WitCallLowerer>(
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
