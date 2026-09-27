use super::super::super::instruction::Instruction;
use super::super::BlockId;
use super::super::{PendingFree, WitCallLowerer};
use crate::BackendError;
use crate::abi::canonical::{CanonicalType, flatten, payload_cases};
use crate::abi::layout::{self, MemorySlot, SlotKind, discriminant_width};
use crate::abi::{self};
use crate::types::{MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

pub(super) fn write_parameter_record<L: WitCallLowerer>(
    lowerer: &mut L,
    params: &[CanonicalType],
    flattened: &[ValueId],
    output: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    let layout = abi::layout::record_layout(params.iter().map(abi::layout::parameter_layout))
        .ok_or_else(|| unsupported_parameter(span))?;
    let offsets = abi::layout::record_fields(params.iter().map(abi::layout::parameter_layout))
        .ok_or_else(|| unsupported_parameter(span))?;
    if layout.size == 0 || offsets.len() != params.len() {
        return Err(unsupported_parameter(span));
    }
    let address = allocate_parameter_area(lowerer, layout.size, layout.align, current, span)?;
    let mut current = current;
    let mut index = 0;
    for (param, (offset, _)) in params.iter().zip(&offsets) {
        let count = flatten(param).len();
        let Some(values) = flattened.get(index..index + count) else {
            return Err(unsupported_parameter(span));
        };
        current = store_parameter(lowerer, address, *offset, param, values, current, span)?;
        index += count;
    }
    if index != flattened.len() {
        return Err(unsupported_parameter(span));
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
        elements: None,
    });
    output.push(address);
    Ok(current)
}

/// Stores one canonical value at `offset` within the parameter record. A
/// variant writes its discriminant and a switch stores the selected case's
/// payload in the shared payload region using that case's own layout.
fn store_parameter<L: WitCallLowerer>(
    lowerer: &mut L,
    base: ValueId,
    offset: u32,
    ty: &CanonicalType,
    values: &[ValueId],
    current: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    match ty {
        CanonicalType::Option(_) | CanonicalType::Result { .. } | CanonicalType::Variant(_) => {
            let cases = payload_cases(ty).ok_or_else(|| unsupported_parameter(span))?;
            let Some((tag, payload_values)) = values.split_first() else {
                return Err(unsupported_parameter(span));
            };
            let payload_offset = layout::variant_payload_offset(&cases)
                .ok_or_else(|| unsupported_parameter(span))?;
            store_slot(
                lowerer,
                base,
                *tag,
                MemorySlot {
                    offset,
                    kind: slot_for_discriminant(cases.len()),
                },
                current,
                span,
            )?;
            let case_blocks = cases
                .iter()
                .map(|_| lowerer.wit_new_block(Vec::new()))
                .collect::<Vec<_>>();
            let default = lowerer.wit_new_block(Vec::new());
            let merge = lowerer.wit_new_block(Vec::new());
            lowerer.wit_switch(
                current,
                *tag,
                case_blocks
                    .iter()
                    .enumerate()
                    .map(|(index, block)| (index as i32, *block))
                    .collect(),
                default,
                span,
            )?;
            lowerer.wit_jump(default, merge, Vec::new(), span)?;
            for (index, case) in cases.iter().enumerate() {
                let block = case_blocks[index];
                let Some(case_ty) = case else {
                    lowerer.wit_jump(block, merge, Vec::new(), span)?;
                    continue;
                };
                let count = flatten(case_ty).len();
                let case_values = payload_values
                    .get(..count)
                    .ok_or_else(|| unsupported_parameter(span))?;
                let end = store_parameter(
                    lowerer,
                    base,
                    offset + payload_offset,
                    case_ty,
                    case_values,
                    block,
                    span,
                )?;
                lowerer.wit_jump(end, merge, Vec::new(), span)?;
            }
            Ok(merge)
        }
        CanonicalType::Record(fields) => {
            let offsets = layout::record_fields(
                fields
                    .iter()
                    .map(|field| layout::parameter_layout(&field.ty)),
            )
            .ok_or_else(|| unsupported_parameter(span))?;
            let mut index = 0;
            let mut current = current;
            for (field, (field_offset, _)) in fields.iter().zip(&offsets) {
                let count = flatten(&field.ty).len();
                let field_values = values
                    .get(index..index + count)
                    .ok_or_else(|| unsupported_parameter(span))?;
                current = store_parameter(
                    lowerer,
                    base,
                    offset + field_offset,
                    &field.ty,
                    field_values,
                    current,
                    span,
                )?;
                index += count;
            }
            Ok(current)
        }
        _ => {
            let field_layout =
                layout::parameter_layout(ty).ok_or_else(|| unsupported_parameter(span))?;
            if field_layout.slots.len() != values.len() {
                return Err(unsupported_parameter(span));
            }
            for (slot, value) in field_layout.slots.iter().zip(values) {
                store_slot(
                    lowerer,
                    base,
                    *value,
                    MemorySlot {
                        offset: offset + slot.offset,
                        kind: slot.kind,
                    },
                    current,
                    span,
                )?;
            }
            Ok(current)
        }
    }
}

fn slot_for_discriminant(cases: usize) -> SlotKind {
    match discriminant_width(cases) {
        1 => SlotKind::Byte,
        2 => SlotKind::Half,
        _ => SlotKind::Word,
    }
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
