use super::super::BlockId;
use super::super::instruction::Instruction;
use super::WitCallLowerer;
use crate::BackendError;
use crate::abi::{self, WasiImport};
use crate::mir::{NumericOp, UnaryOp};
use crate::types::{MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

pub(super) fn lower_parameters<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    source_signature: &abi::SourceSignature,
    arguments: &[ValueId],
    flat: &mut Vec<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if source_signature.parameters.len() != arguments.len()
        || import.param_kinds.len() != arguments.len()
    {
        return Err(vec![BackendError::new(
            "P9 MIR lowering",
            span,
            "WIT parameter count disagrees with the source call signature",
        )]);
    }
    for ((argument, source), kind) in arguments
        .iter()
        .zip(&source_signature.parameters)
        .zip(&import.param_kinds)
    {
        lower_parameter(lowerer, *argument, source, kind, flat, current, span)?;
    }
    Ok(())
}

fn lower_parameter<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    source: &abi::SourceType,
    kind: &abi::WasiParamKind,
    flat: &mut Vec<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    match kind {
        abi::WasiParamKind::Integer32
        | abi::WasiParamKind::Boolean
        | abi::WasiParamKind::Char
        | abi::WasiParamKind::Float64
        | abi::WasiParamKind::Handle
        | abi::WasiParamKind::Enum { .. } => flat.push(argument),
        abi::WasiParamKind::Float32 => {
            let narrowed = lowerer.fresh_wit_value(ValueType::F32);
            lowerer.append_wit_instruction(
                current,
                Instruction::UnaryPrimitive {
                    destination: narrowed,
                    op: UnaryOp::F64ToF32,
                    value: argument,
                    span,
                },
                span,
            )?;
            flat.push(narrowed);
        }
        abi::WasiParamKind::Scalar64 { signed } => {
            let wide = lowerer.fresh_wit_value(ValueType::I64);
            lowerer.append_wit_instruction(
                current,
                Instruction::WidenI64 {
                    destination: wide,
                    value: argument,
                    signed: *signed,
                    span,
                },
                span,
            )?;
            flat.push(wide);
        }
        abi::WasiParamKind::Flags { names } => {
            lower_flags(lowerer, argument, source, names, flat, current, span)?;
        }
        abi::WasiParamKind::List => {
            let length = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::Load {
                    destination: length,
                    address: argument,
                    memory: MemoryId(0),
                    offset: 0,
                    span,
                },
                span,
            )?;
            let four = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::Constant {
                    destination: four,
                    value: 4,
                    span,
                },
                span,
            )?;
            let bytes = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::Primitive {
                    destination: bytes,
                    op: NumericOp::I32Add,
                    left: argument,
                    right: four,
                    span,
                },
                span,
            )?;
            flat.push(bytes);
            flat.push(length);
        }
        abi::WasiParamKind::Record { fields } => {
            let abi::SourceType::Record {
                fields: source_fields,
            } = source
            else {
                return Err(unsupported_parameter(span));
            };
            if fields.len() != source_fields.len() {
                return Err(unsupported_parameter(span));
            }
            for field in fields {
                let source_label = abi::source_field_name(&field.name);
                let Some((index, (_, source_field))) = source_fields
                    .iter()
                    .enumerate()
                    .find(|(_, (label, _))| label == &source_label)
                else {
                    return Err(unsupported_parameter(span));
                };
                let value = lowerer.wit_product_field(current, argument, index as u32, span)?;
                lower_parameter(
                    lowerer,
                    value,
                    source_field,
                    &field.kind,
                    flat,
                    current,
                    span,
                )?;
            }
        }
        abi::WasiParamKind::Unsupported => return Err(unsupported_parameter(span)),
    }
    Ok(())
}

fn lower_flags<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    source: &abi::SourceType,
    names: &[String],
    flat: &mut Vec<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let abi::SourceType::Record { fields } = source else {
        return Err(unsupported_parameter(span));
    };
    if names.len() != fields.len()
        || fields
            .iter()
            .any(|(_, field)| !matches!(field.as_ref(), abi::SourceType::Boolean))
    {
        return Err(unsupported_parameter(span));
    }
    for word_names in names.chunks(32) {
        let mut word = lowerer.fresh_wit_value(ValueType::I32);
        lowerer.append_wit_instruction(
            current,
            Instruction::Constant {
                destination: word,
                value: 0,
                span,
            },
            span,
        )?;
        for (bit, name) in word_names.iter().enumerate() {
            let label = abi::source_field_name(name);
            let Some((field_index, _)) = fields
                .iter()
                .enumerate()
                .find(|(_, (source_label, _))| source_label == &label)
            else {
                return Err(unsupported_parameter(span));
            };
            let boolean = lowerer.wit_product_field(current, argument, field_index as u32, span)?;
            let integer = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::UnaryPrimitive {
                    destination: integer,
                    op: UnaryOp::BoolToI32,
                    value: boolean,
                    span,
                },
                span,
            )?;
            let bit_value = if bit == 0 {
                integer
            } else {
                let shift = lowerer.fresh_wit_value(ValueType::I32);
                lowerer.append_wit_instruction(
                    current,
                    Instruction::Constant {
                        destination: shift,
                        value: bit as i32,
                        span,
                    },
                    span,
                )?;
                let shifted = lowerer.fresh_wit_value(ValueType::I32);
                lowerer.append_wit_instruction(
                    current,
                    Instruction::Primitive {
                        destination: shifted,
                        op: NumericOp::I32Shl,
                        left: integer,
                        right: shift,
                        span,
                    },
                    span,
                )?;
                shifted
            };
            let combined = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::Primitive {
                    destination: combined,
                    op: NumericOp::I32Or,
                    left: word,
                    right: bit_value,
                    span,
                },
                span,
            )?;
            word = combined;
        }
        flat.push(word);
    }
    Ok(())
}

fn unsupported_parameter(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        "this WIT parameter shape is not supported by the source ABI",
    )]
}
