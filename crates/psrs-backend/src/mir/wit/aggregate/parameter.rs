use super::*;
use crate::mir::NumericOp;
use crate::mir::wit::PendingFree;
use crate::mir::wit::parameters::lower_parameter;

/// Lowers a mapped aggregate parameter by branching on its source tag. The tag
/// is pushed first, then the joined payload slots are produced by a merge block
/// that every case reaches with the payload flattened or zero-padded.
#[allow(clippy::too_many_arguments)]
pub(in crate::mir) fn lower_variant_parameter<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    guest: &GuestLayout,
    kind: &CanonicalType,
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    let GuestLayout::Variant { repr, cases } = guest else {
        return Err(unsupported(span));
    };
    let repr = *repr;
    let case_kinds = payload_cases(kind).ok_or_else(|| unsupported(span))?;
    let joined = joined_payload_types(kind).ok_or_else(|| unsupported(span))?;

    let tag = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.wit_variant_tag(current, tag, repr, argument, span)?;
    // A canonical `result` swaps its discriminant relative to the source
    // `Either` ([DEC-13]): the guest constructors are `Left` (err) then `Right`
    // (ok), but the canonical discriminant is `[ok, err]`. Push the canonical
    // discriminant; the switch below stays keyed by the guest tag.
    let canonical = canonical_tag(lowerer, kind, tag, current, span)?;
    flat.push(canonical);

    let case_blocks = case_kinds
        .iter()
        .map(|_| lowerer.wit_new_block(Vec::new()))
        .collect::<Vec<_>>();
    let default = lowerer.wit_new_block(Vec::new());
    let merge_parameters = joined
        .iter()
        .map(|ty| lowerer.fresh_wit_value(*ty))
        .collect::<Vec<_>>();
    let merge = lowerer.wit_new_block(merge_parameters.clone());
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

    for (source_index, _) in case_kinds.iter().enumerate() {
        let block = case_blocks[source_index];
        // The switch is keyed by the guest tag; the selected payload is the
        // canonical case that guest tag maps to.
        let case = case_kinds[canonical_case_for_tag(kind, source_index)];
        let case_field = cases.get(source_index).and_then(|case| case.fields.first());
        let (payload, case_field) = match (case, case_field) {
            (Some(payload), Some(case_field)) => (payload, case_field),
            _ => {
                let zeros = zero_arguments(lowerer, &joined, block, span)?;
                lowerer.wit_jump(block, merge, zeros, span)?;
                continue;
            }
        };
        let field_type = lowerer
            .wit_case_field_type(repr, source_index as u32, 0)
            .ok_or_else(|| unsupported(span))?;
        let erased = lowerer.fresh_wit_value(field_type);
        lowerer.wit_variant_get(block, erased, repr, source_index as u32, 0, argument, span)?;
        let (value, payload_guest) =
            recover_payload(lowerer, erased, case_field, payload, block, span)?;
        let mut case_flat = Vec::new();
        let end = lower_parameter(
            lowerer,
            value,
            Some(&payload_guest),
            payload,
            &mut case_flat,
            frees,
            block,
            span,
        )?;
        let arguments = pad_to(lowerer, case_flat, &joined, end, span)?;
        lowerer.wit_jump(end, merge, arguments, span)?;
    }
    flat.extend(merge_parameters);
    Ok(merge)
}

/// Rewrites a guest constructor tag into its canonical discriminant. Only
/// `result` swaps (`Left`/err is tag 0 in source, discriminant 1 canonically);
/// `option`, `variant`, and `enum` keep tag order.
fn canonical_tag<L: WitCallLowerer>(
    lowerer: &mut L,
    kind: &CanonicalType,
    tag: ValueId,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    if !swaps_case_tags(kind) {
        return Ok(tag);
    }
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
    let swapped = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        block,
        Instruction::Primitive {
            destination: swapped,
            op: NumericOp::I32Xor,
            left: tag,
            right: one,
            span,
        },
        span,
    )?;
    Ok(swapped)
}

fn pad_to<L: WitCallLowerer>(
    lowerer: &mut L,
    mut values: Vec<ValueId>,
    joined: &[ValueType],
    block: BlockId,
    span: TextRange,
) -> Result<Vec<ValueId>, Vec<BackendError>> {
    if values.len() > joined.len() {
        return Err(unsupported(span));
    }
    while values.len() < joined.len() {
        values.push(zero_argument(lowerer, joined[values.len()], block, span)?);
    }
    Ok(values)
}

fn zero_arguments<L: WitCallLowerer>(
    lowerer: &mut L,
    joined: &[ValueType],
    block: BlockId,
    span: TextRange,
) -> Result<Vec<ValueId>, Vec<BackendError>> {
    joined
        .iter()
        .map(|ty| zero_argument(lowerer, *ty, block, span))
        .collect()
}

fn zero_argument<L: WitCallLowerer>(
    lowerer: &mut L,
    ty: ValueType,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    match ty {
        ValueType::I32 | ValueType::Boolean => {
            let destination = lowerer.fresh_wit_value(ty);
            lowerer.append_wit_instruction(
                block,
                Instruction::Constant {
                    destination,
                    value: 0,
                    span,
                },
                span,
            )?;
            Ok(destination)
        }
        ValueType::F64 => {
            let destination = lowerer.fresh_wit_value(ValueType::F64);
            lowerer.append_wit_instruction(
                block,
                Instruction::NumberConstant {
                    destination,
                    value: "0".into(),
                    span,
                },
                span,
            )?;
            Ok(destination)
        }
        ValueType::I64 => {
            let zero = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                block,
                Instruction::Constant {
                    destination: zero,
                    value: 0,
                    span,
                },
                span,
            )?;
            let destination = lowerer.fresh_wit_value(ValueType::I64);
            lowerer.append_wit_instruction(
                block,
                Instruction::WidenI64 {
                    destination,
                    value: zero,
                    signed: false,
                    span,
                },
                span,
            )?;
            Ok(destination)
        }
        _ => Err(unsupported(span)),
    }
}
