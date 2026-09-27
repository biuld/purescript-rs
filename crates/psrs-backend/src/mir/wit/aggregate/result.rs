use super::*;
use crate::abi::WasiImport;
use crate::abi::WasiResultKind;
use crate::cc::Signature;
use crate::types::RefType;

/// Lowers a mapped aggregate result. The call writes the discriminant and the
/// joined payload into the return area; the lowering branches on the
/// discriminant, reads the selected payload, and builds the source value.
#[allow(clippy::too_many_arguments)]
pub(in crate::mir) fn lower_variant_result<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    signature: &Signature,
    destination: ValueId,
    arguments: Vec<ValueId>,
    retptr: Option<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    let address = retptr.ok_or_else(|| unsupported(span))?;
    let repr = shape_repr(&signature.result).ok_or_else(|| unsupported(span))?;
    let cases = result_cases(&import.result_kind).ok_or_else(|| unsupported(span))?;
    let payload_offset =
        abi::layout::variant_payload_offset(&cases).ok_or_else(|| unsupported(span))?;

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
        let fields = match case {
            Some(payload) => vec![read_payload(
                lowerer,
                payload,
                address,
                payload_offset,
                block,
                span,
            )?],
            None => Vec::new(),
        };
        let built = lowerer.fresh_wit_value(aggregate_type());
        lowerer.wit_variant_new(block, built, repr, index as u32, fields, span)?;
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

fn result_cases(kind: &WasiResultKind) -> Option<Vec<Option<&WasiParamKind>>> {
    Some(match kind {
        WasiResultKind::Option { payload } => vec![None, Some(payload)],
        WasiResultKind::ValueResult { ok, err } => vec![Some(ok), Some(err)],
        WasiResultKind::Variant { cases } => {
            cases.iter().map(|case| case.kind.as_deref()).collect()
        }
        _ => return None,
    })
}
