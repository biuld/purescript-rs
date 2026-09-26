use super::super::BlockId;
use super::super::instruction::Instruction;
use super::{PendingFree, WitCallLowerer};
use crate::BackendError;
use crate::abi::{self, WasiImport};
use crate::cc::{RefShape, Reference, ValueShape};
use crate::mir::{NumericOp, UnaryOp};
use crate::types::{MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

mod indirect;
mod narrow;
mod primitive;

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_parameters<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    signature: &crate::cc::Signature,
    arguments: &[ValueId],
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if signature.parameters.len() != arguments.len() {
        return Err(parameter_count(span));
    }
    let mut flattened = Vec::new();
    // A primitive import of an aggregate (`option<string>` as `Int -> String`)
    // has a different source arity than `param_kinds`. Flatten each primitive
    // in order. Nullary enums, closed records, and flags stay on the zip path.
    if signature.parameters.len() != import.param_kinds.len() {
        primitive::lower_primitive_parameters(
            lowerer,
            import,
            signature,
            arguments,
            &mut flattened,
            frees,
            current,
            span,
        )?;
        flat.extend(flattened);
        return Ok(());
    }
    for ((argument, shape), kind) in arguments
        .iter()
        .zip(&signature.parameters)
        .zip(&import.param_kinds)
    {
        lower_parameter(
            lowerer,
            *argument,
            shape,
            kind,
            &mut flattened,
            frees,
            current,
            span,
        )?;
    }
    if import.has_indirect_parameters() {
        indirect::write_parameter_record(
            lowerer,
            &import.param_kinds,
            &flattened,
            flat,
            frees,
            current,
            span,
        )?;
    } else {
        flat.extend(flattened);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_parameter<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    shape: &ValueShape,
    kind: &abi::WasiParamKind,
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    match kind {
        abi::WasiParamKind::Integer32
        | abi::WasiParamKind::Boolean
        | abi::WasiParamKind::Char
        | abi::WasiParamKind::Float64
        | abi::WasiParamKind::Enum { .. } => flat.push(argument),
        abi::WasiParamKind::Handle(handle) => {
            // `own<T>` transfers the index; the host lifts it, so do not drop it too.
            if handle.mode == abi::HandleMode::Own {
                lowerer.transfer_owned(argument);
            }
            flat.push(argument);
        }
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
        abi::WasiParamKind::IntegerNarrow { bits, signed } => {
            let narrowed =
                narrow::narrow_integer(lowerer, argument, *bits, *signed, current, span)?;
            flat.push(narrowed);
        }
        abi::WasiParamKind::Flags { names } => {
            lower_flags(lowerer, argument, shape, names, flat, current, span)?;
        }
        abi::WasiParamKind::List => {
            // Transcode the GC string's UTF-16 into a fresh UTF-8 linear
            // buffer. The helper returns the address of a length prefix; the
            // canonical exchange passes the payload pointer and byte length.
            let prefix = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::Call {
                    destination: prefix,
                    function: crate::abi::STRING_TO_BYTES_SYMBOL,
                    arguments: vec![argument],
                    span,
                },
                span,
            )?;
            let length = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::Load {
                    destination: length,
                    address: prefix,
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
                    left: prefix,
                    right: four,
                    span,
                },
                span,
            )?;
            flat.push(bytes);
            flat.push(length);
            // The transcode buffer is call-local: free it after the call returns.
            frees.push(PendingFree {
                pointer: bytes,
                length,
                align: 1,
                string_elements: None,
            });
        }
        abi::WasiParamKind::ValueList { element } => {
            super::lists::write_value_list(
                lowerer, argument, shape, element, flat, frees, current, span,
            )?;
        }
        abi::WasiParamKind::Record { fields } => {
            let (product, labels) = product_of(lowerer, shape, span)?;
            if fields.len() != labels.len() {
                return Err(unsupported_parameter(span));
            }
            for field in fields {
                let source_label = abi::source_field_name(&field.name);
                let Some(index) = labels.iter().position(|label| label == &source_label) else {
                    return Err(unsupported_parameter(span));
                };
                let value = lowerer.wit_product_field(current, argument, index as u32, span)?;
                lower_parameter(
                    lowerer,
                    value,
                    &product[index],
                    &field.kind,
                    flat,
                    frees,
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
    shape: &ValueShape,
    names: &[String],
    flat: &mut Vec<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let (_, labels) = product_of(lowerer, shape, span)?;
    if names.len() != labels.len() {
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
            let Some(field_index) = labels
                .iter()
                .position(|source_label| source_label == &label)
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

fn product_of<L: WitCallLowerer>(
    lowerer: &L,
    shape: &ValueShape,
    span: TextRange,
) -> Result<(Vec<ValueShape>, Vec<String>), Vec<BackendError>> {
    let ValueShape::Reference(Reference {
        heap: RefShape::Repr(repr),
        ..
    }) = shape
    else {
        return Err(unsupported_parameter(span));
    };
    lowerer
        .wit_product(*repr)
        .ok_or_else(|| unsupported_parameter(span))
}

fn parameter_count(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        "WIT parameter count disagrees with the source call signature",
    )]
}

fn unsupported_parameter(span: TextRange) -> Vec<BackendError> {
    vec![BackendError::new(
        "P9 MIR lowering",
        span,
        "this WIT parameter shape is not supported by the source ABI",
    )]
}
