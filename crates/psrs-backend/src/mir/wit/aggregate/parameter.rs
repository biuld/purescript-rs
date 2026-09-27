use super::*;
use crate::mir::wit::PendingFree;
use crate::mir::wit::parameters::lower_parameter;

/// Lowers a mapped aggregate parameter by branching on its source tag. The tag
/// is pushed first, then the joined payload slots are produced by a merge block
/// that every case reaches with the payload flattened or zero-padded.
#[allow(clippy::too_many_arguments)]
pub(in crate::mir) fn lower_variant_parameter<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    shape: &ValueShape,
    kind: &WasiParamKind,
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    let repr = shape_repr(shape).ok_or_else(|| unsupported(span))?;
    let cases = payload_cases(kind).ok_or_else(|| unsupported(span))?;
    let joined = joined_payload_types(kind).ok_or_else(|| unsupported(span))?;

    let tag = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.wit_variant_tag(current, tag, repr, argument, span)?;
    flat.push(tag);

    let case_blocks = cases
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

    for (index, case) in cases.iter().enumerate() {
        let block = case_blocks[index];
        let Some(payload) = case else {
            let zeros = zero_arguments(lowerer, &merge_parameters, block, span)?;
            lowerer.wit_jump(block, merge, zeros, span)?;
            continue;
        };
        let field_type = lowerer
            .wit_case_field_type(repr, index as u32, 0)
            .ok_or_else(|| unsupported(span))?;
        let erased = lowerer.fresh_wit_value(field_type);
        lowerer.wit_variant_get(block, erased, repr, index as u32, 0, argument, span)?;
        let value = recover_payload(lowerer, erased, payload, block, span)?;
        let payload_shape = direct_shape(payload).ok_or_else(|| unsupported(span))?;
        let mut case_flat = Vec::new();
        let end = lower_parameter(
            lowerer,
            value,
            &payload_shape,
            payload,
            &mut case_flat,
            frees,
            block,
            span,
        )?;
        let arguments = pad_to(lowerer, case_flat, &merge_parameters, end, span)?;
        lowerer.wit_jump(end, merge, arguments, span)?;
    }
    flat.extend(merge_parameters);
    Ok(merge)
}

fn pad_to<L: WitCallLowerer>(
    lowerer: &mut L,
    mut values: Vec<ValueId>,
    merge_parameters: &[ValueId],
    block: BlockId,
    span: TextRange,
) -> Result<Vec<ValueId>, Vec<BackendError>> {
    if values.len() > merge_parameters.len() {
        return Err(unsupported(span));
    }
    while values.len() < merge_parameters.len() {
        values.push(zero_argument(lowerer, ValueType::I32, block, span)?);
    }
    Ok(values)
}

fn zero_arguments<L: WitCallLowerer>(
    lowerer: &mut L,
    merge_parameters: &[ValueId],
    block: BlockId,
    span: TextRange,
) -> Result<Vec<ValueId>, Vec<BackendError>> {
    merge_parameters
        .iter()
        .map(|_| zero_argument(lowerer, ValueType::I32, block, span))
        .collect()
}

fn zero_argument<L: WitCallLowerer>(
    lowerer: &mut L,
    ty: ValueType,
    block: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    if ty != ValueType::I32 {
        return Err(unsupported(span));
    }
    let destination = lowerer.fresh_wit_value(ValueType::I32);
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
