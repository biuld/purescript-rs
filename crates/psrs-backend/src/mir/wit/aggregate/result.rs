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
    guest: &GuestLayout,
    destination: ValueId,
    arguments: Vec<ValueId>,
    retptr: Option<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    let address = retptr.ok_or_else(|| unsupported(span))?;
    let GuestLayout::Variant { repr, cases } = guest else {
        return Err(unsupported(span));
    };
    let repr = *repr;
    let result = import
        .canonical_result
        .as_ref()
        .ok_or_else(|| unsupported(span))?;
    let case_kinds = payload_cases(result).ok_or_else(|| unsupported(span))?;
    let payload_offset =
        abi::layout::variant_payload_offset(&case_kinds).ok_or_else(|| unsupported(span))?;
    // A canonical `result` swaps its tag relative to the source `Either`: the
    // canonical discriminant is `[ok, err]`, but the guest constructors are
    // `Left` (err) then `Right` (ok) ([DEC-13]). The return-area discriminant is
    // still keyed by canonical index; the built guest case uses the source tag.
    let source_tags = (0..case_kinds.len())
        .map(|index| source_tag_for_case(result, index))
        .collect::<Vec<_>>();
    // Each case's projected field carries both the concrete source value and the
    // storage slot; MIR decodes the value and erases it when the slot is erased.
    let case_fields = source_tags
        .iter()
        .map(|&tag| cases.get(tag).and_then(|case| case.fields.first()))
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

    let case_blocks = case_kinds
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
    for (index, case) in case_kinds.iter().enumerate() {
        let block = case_blocks[index];
        let (field, block) = match (case, case_fields[index]) {
            (Some(payload), Some(case_field)) => build_payload(
                lowerer,
                payload,
                &case_field.value,
                case_field.stored,
                address,
                payload_offset,
                block,
                span,
            )?,
            // An absent WIT payload is a nullary case; its source case still
            // carries a `Unit` field (for example `Left ()` of `Either Unit E`),
            // decoded as the zero integer into the declared storage slot.
            (None, Some(case_field)) => absent_payload(lowerer, case_field, block, span)?,
            _ => (None, block),
        };
        let built = lowerer.fresh_wit_value(aggregate_type());
        lowerer.wit_variant_new(
            block,
            built,
            repr,
            source_tags[index] as u32,
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

/// Builds the source field of a WIT case whose payload is absent. Such a case
/// maps to a nullary WIT case and a `Unit` source field, so the field value is
/// the zero integer, boxed and erased when its storage slot is erased (like any
/// other `Unit` field).
fn absent_payload<L: WitCallLowerer>(
    lowerer: &mut L,
    case_field: &crate::cc::Field,
    block: BlockId,
    span: TextRange,
) -> Result<(Option<ValueId>, BlockId), Vec<BackendError>> {
    let value = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        block,
        Instruction::Constant {
            destination: value,
            value: 0,
            span,
        },
        span,
    )?;
    let value = if is_erased(case_field.stored) {
        box_scalar(lowerer, value, block, span)?
    } else {
        value
    };
    Ok((Some(value), block))
}

/// Lowers a top-level record result. The call writes the record into the
/// canonical return area; the lowering reads each field and builds the source
/// record, then passes it to the merge block as the call's result value.
#[allow(clippy::too_many_arguments)]
pub(in crate::mir) fn lower_record_result<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    kind: &CanonicalType,
    guest: &GuestLayout,
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
    // The projection names the concrete shape of each field, which a nested
    // erased aggregate needs.
    let stored = guest.shape();
    let (value, block) = build_payload(lowerer, kind, guest, stored, address, 0, current, span)?;
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
