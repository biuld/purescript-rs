use super::decode::build_payload;
use super::*;
use crate::abi::WasiImport;
use crate::types::RefType;

/// Lowers a mapped aggregate result. The call writes the discriminant and the
/// joined payload into the return area; the lowering branches on the
/// discriminant, reads the selected payload, and builds the source value.
#[allow(clippy::too_many_arguments)]
pub(in crate::mir) fn lower_variant_result<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    shape: &ValueShape,
    guest: Option<&GuestLayout>,
    decode: Option<&GuestLayout>,
    destination: ValueId,
    arguments: Vec<ValueId>,
    retptr: Option<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    let address = retptr.ok_or_else(|| unsupported(span))?;
    let repr = shape_repr(shape).ok_or_else(|| unsupported(span))?;
    let result = import
        .canonical_result
        .as_ref()
        .ok_or_else(|| unsupported(span))?;
    let cases = payload_cases(result).ok_or_else(|| unsupported(span))?;
    let payload_offset =
        abi::layout::variant_payload_offset(&cases).ok_or_else(|| unsupported(span))?;
    // The stored field shape comes from the abstract guest layout; the concrete
    // decode shape comes from the result's decode tree when the storage field is
    // erased. A `Repr` decode shape resolves the nested source aggregate so MIR
    // can decode it before erasing the reference into the field.
    let field_shapes = (0..cases.len())
        .map(|index| match guest {
            Some(GuestLayout::Variant { cases, .. }) => cases
                .get(index)
                .and_then(|case| case.fields.first().copied()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let decode_shapes = (0..cases.len())
        .map(|index| match decode {
            Some(GuestLayout::Variant { cases, .. }) => cases
                .get(index)
                .and_then(|case| case.fields.first().copied())
                .or(field_shapes[index]),
            _ => field_shapes[index],
        })
        .collect::<Vec<_>>();
    let layouts = decode_shapes
        .iter()
        .map(|shape| shape.and_then(|shape| lowerer.wit_guest_layout(shape)))
        .collect::<Vec<_>>();

    lowerer.append_wit_instruction(
        current,
        Instruction::CallVoid {
            function: import.symbol,
            arguments,
            span,
        },
        span,
    )?;
    let tag = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Load8U {
            destination: tag,
            address,
            memory: MemoryId(0),
            offset: 0,
            span,
        },
        span,
    )?;

    let case_blocks = cases
        .iter()
        .map(|_| lowerer.wit_new_block(Vec::new()))
        .collect::<Vec<_>>();
    let default = lowerer.wit_new_block(Vec::new());
    let merge = lowerer.wit_new_block(vec![destination]);
    lowerer.wit_switch(
        current,
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

    let supertype = lowerer
        .wit_repr_index(repr)
        .ok_or_else(|| unsupported(span))?;
    let supertype_reference = RefType {
        nullable: false,
        heap: HeapType::Index(supertype),
    };
    for (index, case) in cases.iter().enumerate() {
        let block = case_blocks[index];
        let (field, block) = match case {
            Some(payload) => build_payload(
                lowerer,
                payload,
                layouts[index].as_ref(),
                field_shapes[index],
                address,
                payload_offset,
                block,
                span,
            )?,
            None => (None, block),
        };
        let built = lowerer.fresh_wit_value(aggregate_type());
        lowerer.wit_variant_new(
            block,
            built,
            repr,
            index as u32,
            field.into_iter().collect(),
            span,
        )?;
        let cast = lowerer.fresh_wit_value(ValueType::Ref(supertype_reference));
        lowerer.append_wit_instruction(
            block,
            Instruction::RefCast {
                destination: cast,
                value: built,
                reference: supertype_reference,
                span,
            },
            span,
        )?;
        lowerer.wit_jump(block, merge, vec![cast], span)?;
    }
    Ok(merge)
}

/// Lowers a top-level record result. The call writes the record into the
/// canonical return area; the lowering reads each field and builds the source
/// record, then passes it to the merge block as the call's result value.
#[allow(clippy::too_many_arguments)]
pub(in crate::mir) fn lower_record_result<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    kind: &CanonicalType,
    shape: &ValueShape,
    guest: Option<&GuestLayout>,
    decode: Option<&GuestLayout>,
    destination: ValueId,
    arguments: Vec<ValueId>,
    retptr: Option<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    let address = retptr.ok_or_else(|| unsupported(span))?;
    lowerer.append_wit_instruction(
        current,
        Instruction::CallVoid {
            function: import.symbol,
            arguments,
            span,
        },
        span,
    )?;
    // A record's decode tree names the concrete shape of each field, which a
    // nested erased aggregate needs. Fall back to the abstract guest layout
    // when the result is not an aggregate.
    let concrete = decode.or(guest);
    let (value, block) = build_payload(
        lowerer,
        kind,
        concrete,
        Some(*shape),
        address,
        0,
        current,
        span,
    )?;
    let value = value.ok_or_else(|| unsupported(span))?;
    let merge = lowerer.wit_new_block(vec![destination]);
    lowerer.wit_jump(block, merge, vec![value], span)?;
    Ok(merge)
}

/// The return pointer for an import whose result is passed indirectly. A small
/// return area uses the fixed scratch region; a larger aggregate return area is
/// allocated through `cabi_realloc` and freed once the result is read.
pub(in crate::mir) fn retptr_buffer<L: WitCallLowerer>(
    lowerer: &mut L,
    area: Option<(u32, u32)>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    if let Some((size, align)) = area
        && size > abi::SCRATCH_SIZE
    {
        let size_value = lowerer.fresh_wit_value(ValueType::I32);
        lowerer.append_wit_instruction(
            current,
            Instruction::Constant {
                destination: size_value,
                value: size as i32,
                span,
            },
            span,
        )?;
        let pointer =
            crate::mir::wit::lists::allocate(lowerer, size_value, align as i32, current, span)?;
        frees.push(PendingFree {
            pointer,
            length: size_value,
            align: align as i32,
            elements: None,
        });
        return Ok(pointer);
    }
    let scratch = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Constant {
            destination: scratch,
            value: abi::PRINT_SCRATCH,
            span,
        },
        span,
    )?;
    Ok(scratch)
}
