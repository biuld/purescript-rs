//! Decodes a mapped aggregate payload from the canonical return area.

use super::collections::{read_flags, read_value_list};
use super::memory::{load, load_discriminant, load_f32, load_f64, load_i64, load8};
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
    let Some(node) = node.filter(|node| !matches!(node, crate::cc::PayloadNode::None)) else {
        return Ok((
            Some(read_direct(lowerer, kind, address, offset, block, span)?),
            block,
        ));
    };
    let shape = node.shape().ok_or_else(|| unsupported(span))?;
    let (value, block) = build_concrete(lowerer, kind, node, address, offset, block, span)?;
    let erased = erase_payload(lowerer, value, shape, block, span)?;
    Ok((Some(erased), block))
}

/// Stores a concrete payload in the erased variant field: a scalar is boxed and
/// a reference is cast to the erased type.
fn erase_payload<L: WitCallLowerer>(
    lowerer: &mut L,
    value: ValueId,
    shape: ValueShape,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    match shape {
        ValueShape::Integer | ValueShape::Boolean => box_scalar(lowerer, value, block, span),
        ValueShape::Number => box_number(lowerer, value, block, span),
        ValueShape::String | ValueShape::Reference(_) => {
            erase_reference(lowerer, value, block, span)
        }
    }
}

/// Reads a payload from the return area as its concrete source value.
#[allow(clippy::too_many_arguments)]
fn build_concrete<L: WitCallLowerer>(
    lowerer: &mut L,
    kind: &WasiParamKind,
    node: &crate::cc::PayloadNode,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<(ValueId, BlockId), Vec<BackendError>> {
    match node {
        crate::cc::PayloadNode::Variant {
            representation,
            cases,
        } if is_variant_kind(kind) => build_variant(
            lowerer,
            kind,
            *representation,
            cases,
            address,
            offset,
            block,
            span,
        ),
        crate::cc::PayloadNode::Record {
            representation,
            fields,
        } => match kind {
            WasiParamKind::Record { fields: wit_fields } => read_record(
                lowerer,
                *representation,
                fields,
                wit_fields,
                address,
                offset,
                block,
                span,
            ),
            WasiParamKind::Flags { names } => Ok((
                read_flags(
                    lowerer,
                    *representation,
                    names,
                    address,
                    offset,
                    block,
                    span,
                )?,
                block,
            )),
            _ => Err(unsupported(span)),
        },
        crate::cc::PayloadNode::List {
            representation,
            element,
        } => match kind {
            WasiParamKind::ValueList {
                element: wit_element,
            } => Ok((
                read_value_list(
                    lowerer,
                    *representation,
                    element,
                    wit_element,
                    address,
                    offset,
                    block,
                    span,
                )?,
                block,
            )),
            _ => Err(unsupported(span)),
        },
        crate::cc::PayloadNode::Value(shape) => Ok((
            read_value_concrete(lowerer, *shape, kind, address, offset, block, span)?,
            block,
        )),
        crate::cc::PayloadNode::None | crate::cc::PayloadNode::Variant { .. } => {
            Err(unsupported(span))
        }
    }
}

fn is_variant_kind(kind: &WasiParamKind) -> bool {
    matches!(
        kind,
        WasiParamKind::Option { .. } | WasiParamKind::Result { .. } | WasiParamKind::Variant { .. }
    )
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
    Ok((merge_value, merge))
}

/// Reads a scalar, string, or number payload as its concrete source value.
#[allow(clippy::too_many_arguments)]
fn read_value_concrete<L: WitCallLowerer>(
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
            read_scalar(lowerer, kind, address, offset, block, span)
        }
        ValueShape::Number => read_number(lowerer, kind, address, offset, block, span),
        ValueShape::String => read_string(lowerer, address, offset, block, span),
        ValueShape::Reference(_) => Err(unsupported(span)),
    }
}

/// Reads an `f32`/`f64` payload as a source `Number`.
fn read_number<L: WitCallLowerer>(
    lowerer: &mut L,
    kind: &WasiParamKind,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    match kind {
        WasiParamKind::Float64 => load_f64(lowerer, address, offset, block, span),
        WasiParamKind::Float32 => {
            let narrow = load_f32(lowerer, address, offset, block, span)?;
            let wide = lowerer.fresh_wit_value(ValueType::F64);
            lowerer.append_wit_instruction(
                block,
                Instruction::UnaryPrimitive {
                    destination: wide,
                    op: UnaryOp::F32ToF64,
                    value: narrow,
                    span,
                },
                span,
            )?;
            Ok(wide)
        }
        _ => Err(unsupported(span)),
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
    if matches!(kind, WasiParamKind::Scalar64 { .. }) {
        let wide = load_i64(lowerer, address, offset, block, span)?;
        let narrowed = lowerer.fresh_wit_value(ValueType::I32);
        lowerer.append_wit_instruction(
            block,
            Instruction::WrapI64 {
                destination: narrowed,
                value: wide,
                span,
            },
            span,
        )?;
        return Ok(narrowed);
    }
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
    Ok(value)
}

/// Builds a GC record from the canonical field slots, recursing into a field
/// that is itself an aggregate.
#[allow(clippy::too_many_arguments)]
fn read_record<L: WitCallLowerer>(
    lowerer: &mut L,
    representation: crate::cc::ReprId,
    node_fields: &[crate::cc::PayloadField],
    wit_fields: &[crate::abi::WasiField],
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<(ValueId, BlockId), Vec<BackendError>> {
    let (product, labels) = lowerer
        .wit_product(representation)
        .ok_or_else(|| unsupported(span))?;
    let layouts = layout::record_fields(
        wit_fields
            .iter()
            .map(|field| layout::parameter_layout(&field.kind)),
    )
    .ok_or_else(|| unsupported(span))?;
    let mut values = vec![None; product.len()];
    let mut block = block;
    for (wit_field, (field_offset, _)) in wit_fields.iter().zip(&layouts) {
        let label = abi::source_field_name(&wit_field.name);
        let index = labels
            .iter()
            .position(|candidate| candidate == &label)
            .ok_or_else(|| unsupported(span))?;
        let field_node = node_fields
            .iter()
            .find(|field| field.name == label)
            .map(|field| &field.node)
            .ok_or_else(|| unsupported(span))?;
        let (value, next) = build_concrete(
            lowerer,
            &wit_field.kind,
            field_node,
            address,
            offset + field_offset,
            block,
            span,
        )?;
        values[index] = Some(value);
        block = next;
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
    Ok((destination, block))
}

/// The fallback path: reads a directly flattenable payload and erases it.
fn read_direct<L: WitCallLowerer>(
    lowerer: &mut L,
    kind: &WasiParamKind,
    address: ValueId,
    offset: u32,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    if matches!(kind, WasiParamKind::List) {
        let value = read_string(lowerer, address, offset, block, span)?;
        return erase_reference(lowerer, value, block, span);
    }
    let scalar = read_scalar(lowerer, kind, address, offset, block, span)?;
    box_scalar(lowerer, scalar, block, span)
}
