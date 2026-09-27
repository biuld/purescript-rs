//! Decodes a mapped aggregate payload from the canonical return area.

use super::*;
use crate::abi::layout::{self, SlotKind};

/// Reads a payload from the return area and erases it into the stored variant
/// field. Returns the erased value and the block the caller continues in.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_payload<L: WitCallLowerer>(
    lowerer: &mut L,
    kind: &WasiParamKind,
    node: Option<&crate::cc::PayloadNode>,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<(Option<ValueId>, BlockId), Vec<BackendError>> {
    if let (
        true,
        Some(crate::cc::PayloadNode::Variant {
            representation,
            cases,
        }),
    ) = (
        matches!(
            kind,
            WasiParamKind::Option { .. }
                | WasiParamKind::Result { .. }
                | WasiParamKind::Variant { .. }
        ),
        node,
    ) {
        let (value, block) = build_variant(
            lowerer,
            kind,
            *representation,
            cases,
            address,
            offset,
            block,
            span,
        )?;
        return Ok((Some(value), block));
    }
    let erased = match node {
        Some(crate::cc::PayloadNode::Value(shape)) => {
            read_value(lowerer, *shape, kind, address, offset, block, span)?
        }
        _ => read_direct(lowerer, kind, address, offset, block, span)?,
    };
    Ok((Some(erased), block))
}

/// Reads a nested source variant from the return area and rebuilds it.
#[allow(clippy::too_many_arguments)]
fn build_variant<L: WitCallLowerer>(
    lowerer: &mut L,
    kind: &WasiParamKind,
    representation: crate::cc::ReprId,
    cases: &[Option<crate::cc::PayloadNode>],
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<(ValueId, BlockId), Vec<BackendError>> {
    let case_kinds = payload_cases(kind).ok_or_else(|| unsupported(span))?;
    let tag = load_discriminant(lowerer, address, offset, case_kinds.len(), block, span)?;
    let payload_offset =
        layout::variant_payload_offset(&case_kinds).ok_or_else(|| unsupported(span))?;
    let case_blocks = cases
        .iter()
        .map(|_| lowerer.wit_new_block(Vec::new()))
        .collect::<Vec<_>>();
    let default = lowerer.wit_new_block(Vec::new());
    let supertype = lowerer
        .wit_repr_index(representation)
        .ok_or_else(|| unsupported(span))?;
    let merge_value = lowerer.fresh_wit_value(ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(supertype),
    }));
    let merge = lowerer.wit_new_block(vec![merge_value]);
    lowerer.wit_switch(
        block,
        tag,
        case_blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (index as i32, *block))
            .collect(),
        default,
        span,
    )?;
    lowerer.wit_jump(default, case_blocks[0], Vec::new(), span)?;

    for (index, (case_kind, case_node)) in case_kinds.iter().zip(cases).enumerate() {
        let block = case_blocks[index];
        let (field, block) = match case_kind {
            Some(case_kind) => build_payload(
                lowerer,
                case_kind,
                case_node.as_ref(),
                address,
                offset + payload_offset,
                block,
                span,
            )?,
            None => (None, block),
        };
        let built = lowerer.fresh_wit_value(aggregate_type());
        lowerer.wit_variant_new(
            block,
            built,
            representation,
            index as u32,
            field.into_iter().collect(),
            span,
        )?;
        let reference = RefType {
            nullable: false,
            heap: HeapType::Index(supertype),
        };
        let cast = lowerer.fresh_wit_value(ValueType::Ref(reference));
        lowerer.append_wit_instruction(
            block,
            Instruction::RefCast {
                destination: cast,
                value: built,
                reference,
                span,
            },
            span,
        )?;
        lowerer.wit_jump(block, merge, vec![cast], span)?;
    }
    // The parent variant stores this payload erased.
    let erased = erase_reference(lowerer, merge_value, merge, span)?;
    Ok((erased, merge))
}

/// Reads a concrete payload shape from the return area and erases it.
#[allow(clippy::too_many_arguments)]
fn read_value<L: WitCallLowerer>(
    lowerer: &mut L,
    shape: ValueShape,
    kind: &WasiParamKind,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    match shape {
        ValueShape::Integer | ValueShape::Boolean => {
            let scalar = read_scalar(lowerer, kind, address, offset, block, span)?;
            box_scalar(lowerer, scalar, block, span)
        }
        ValueShape::String => read_string(lowerer, address, offset, block, span),
        ValueShape::Reference(reference) => match reference.heap {
            RefShape::Repr(repr) => {
                let WasiParamKind::Record { fields } = kind else {
                    return Err(unsupported(span));
                };
                let value = read_record(lowerer, repr, fields, address, offset, block, span)?;
                erase_reference(lowerer, value, block, span)
            }
            _ => Err(unsupported(span)),
        },
        ValueShape::Number => Err(unsupported(span)),
    }
}

/// Reads a scalar payload of a known WIT kind from memory.
fn read_scalar<L: WitCallLowerer>(
    lowerer: &mut L,
    kind: &WasiParamKind,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    if !matches!(
        kind,
        WasiParamKind::Integer32
            | WasiParamKind::IntegerNarrow { .. }
            | WasiParamKind::Boolean
            | WasiParamKind::Char
            | WasiParamKind::Enum { .. }
    ) {
        return Err(unsupported(span));
    }
    let layout = layout::parameter_layout(kind).ok_or_else(|| unsupported(span))?;
    let slot = layout.slots.first().ok_or_else(|| unsupported(span))?;
    match slot.kind {
        SlotKind::Byte => load8(lowerer, address, offset + slot.offset, block, span),
        SlotKind::Word => load(lowerer, address, offset + slot.offset, block, span),
        _ => Err(unsupported(span)),
    }
}

/// Reads a `(pointer, length)` byte list from memory and decodes it.
fn read_string<L: WitCallLowerer>(
    lowerer: &mut L,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let pointer = load(lowerer, address, offset, block, span)?;
    let length = load(lowerer, address, offset + 4, block, span)?;
    let string = lowerer
        .wit_string_index()
        .ok_or_else(|| unsupported(span))?;
    let reference = RefType {
        nullable: false,
        heap: HeapType::Index(string),
    };
    let value = lowerer.fresh_wit_value(ValueType::Ref(reference));
    lowerer.append_wit_instruction(
        block,
        Instruction::Call {
            destination: value,
            function: abi::BYTES_TO_STRING_SYMBOL,
            arguments: vec![pointer, length],
            span,
        },
        span,
    )?;
    free_buffer(lowerer, pointer, length, 1, block, span)?;
    erase_reference(lowerer, value, block, span)
}

/// Builds a GC record from the canonical field slots.
#[allow(clippy::too_many_arguments)]
fn read_record<L: WitCallLowerer>(
    lowerer: &mut L,
    representation: crate::cc::ReprId,
    fields: &[crate::abi::WasiField],
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let (product, labels) = lowerer
        .wit_product(representation)
        .ok_or_else(|| unsupported(span))?;
    let layouts = layout::record_fields(
        fields
            .iter()
            .map(|field| layout::parameter_layout(&field.kind)),
    )
    .ok_or_else(|| unsupported(span))?;
    let mut values = vec![None; product.len()];
    for (field, (field_offset, _)) in fields.iter().zip(&layouts) {
        let label = abi::source_field_name(&field.name);
        let index = labels
            .iter()
            .position(|candidate| candidate == &label)
            .ok_or_else(|| unsupported(span))?;
        values[index] = Some(read_field(
            lowerer,
            &field.kind,
            address,
            offset + field_offset,
            block,
            span,
        )?);
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

/// Reads a directly representable record field (a scalar or byte list).
fn read_field<L: WitCallLowerer>(
    lowerer: &mut L,
    kind: &WasiParamKind,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    if matches!(kind, WasiParamKind::List) {
        let pointer = load(lowerer, address, offset, block, span)?;
        let length = load(lowerer, address, offset + 4, block, span)?;
        let string = lowerer
            .wit_string_index()
            .ok_or_else(|| unsupported(span))?;
        let reference = RefType {
            nullable: false,
            heap: HeapType::Index(string),
        };
        let value = lowerer.fresh_wit_value(ValueType::Ref(reference));
        lowerer.append_wit_instruction(
            block,
            Instruction::Call {
                destination: value,
                function: abi::BYTES_TO_STRING_SYMBOL,
                arguments: vec![pointer, length],
                span,
            },
            span,
        )?;
        free_buffer(lowerer, pointer, length, 1, block, span)?;
        return Ok(value);
    }
    read_scalar(lowerer, kind, address, offset, block, span)
}

/// The Stage 2 path: reads a directly flattenable payload and erases it.
fn read_direct<L: WitCallLowerer>(
    lowerer: &mut L,
    kind: &WasiParamKind,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    if matches!(kind, WasiParamKind::List) {
        return read_string(lowerer, address, offset, block, span);
    }
    let scalar = read_scalar(lowerer, kind, address, offset, block, span)?;
    box_scalar(lowerer, scalar, block, span)
}

fn load_discriminant<L: WitCallLowerer>(
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

fn load<L: WitCallLowerer>(
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

fn load8<L: WitCallLowerer>(
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
