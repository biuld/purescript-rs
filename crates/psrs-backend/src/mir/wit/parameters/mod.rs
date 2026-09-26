use super::super::BlockId;
use super::super::instruction::Instruction;
use super::{PendingFree, WitCallLowerer};
use crate::BackendError;
use crate::abi::{self, WasiImport};
use crate::mir::{NumericOp, UnaryOp};
use crate::types::{MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

mod indirect;
mod primitive;

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_parameters<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    source_signature: &abi::SourceSignature,
    arguments: &[ValueId],
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    if source_signature.parameters.len() != arguments.len() {
        return Err(parameter_count(span));
    }
    let mut flattened = Vec::new();
    // A primitive import of an aggregate (`option<string>` as `Int -> String`)
    // has a different source arity than `param_kinds`. Flatten each primitive
    // in order. Nullary enums, closed records, and flags stay on the zip path.
    if primitive_flat_call(import, source_signature) {
        primitive::lower_primitive_parameters(
            lowerer,
            import,
            source_signature,
            arguments,
            &mut flattened,
            frees,
            current,
            span,
        )?;
        flat.extend(flattened);
        return Ok(());
    }
    if import.param_kinds.len() != arguments.len() {
        return Err(parameter_count(span));
    }
    for ((argument, source), kind) in arguments
        .iter()
        .zip(&source_signature.parameters)
        .zip(&import.param_kinds)
    {
        lower_parameter(
            lowerer,
            *argument,
            source,
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
    source: &abi::SourceType,
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
        abi::WasiParamKind::IntegerNarrow { bits, signed } => {
            let narrowed = narrow_integer(lowerer, argument, *bits, *signed, current, span)?;
            flat.push(narrowed);
        }
        abi::WasiParamKind::Flags { names } => {
            lower_flags(lowerer, argument, source, names, flat, current, span)?;
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
            });
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

fn primitive_flat_call(import: &WasiImport, signature: &abi::SourceSignature) -> bool {
    abi::is_primitive_signature(&signature.parameters, &signature.result)
        && signature.parameters.len() != import.param_kinds.len()
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

/// Masks a source `Int` to a WIT narrow integer width. A signed narrow type is
/// then sign-extended so the canonical `i32` is an in-range signed value.
fn narrow_integer<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    bits: u8,
    signed: bool,
    current: BlockId,
    span: TextRange,
) -> Result<ValueId, Vec<BackendError>> {
    let mask = (1i32 << bits) - 1;
    let mask_value = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Constant {
            destination: mask_value,
            value: mask,
            span,
        },
        span,
    )?;
    let masked = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Primitive {
            destination: masked,
            op: NumericOp::I32And,
            left: argument,
            right: mask_value,
            span,
        },
        span,
    )?;
    if !signed {
        return Ok(masked);
    }
    let shift = 32 - bits;
    let shift_value = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Constant {
            destination: shift_value,
            value: shift as i32,
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
            left: masked,
            right: shift_value,
            span,
        },
        span,
    )?;
    let sign_extended = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Primitive {
            destination: sign_extended,
            op: NumericOp::I32ShrS,
            left: shifted,
            right: shift_value,
            span,
        },
        span,
    )?;
    Ok(sign_extended)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder {
        next: u32,
        instructions: Vec<Instruction>,
        primitives: Vec<NumericOp>,
    }

    impl WitCallLowerer for Recorder {
        fn fresh_wit_value(&mut self, _ty: ValueType) -> ValueId {
            let id = ValueId(self.next);
            self.next += 1;
            id
        }

        fn append_wit_instruction(
            &mut self,
            _block: BlockId,
            instruction: Instruction,
            _span: TextRange,
        ) -> Result<(), Vec<BackendError>> {
            if let Instruction::Primitive { op, .. } = &instruction {
                self.primitives.push(*op);
            }
            self.instructions.push(instruction);
            Ok(())
        }

        fn wit_product_field(
            &mut self,
            _block: BlockId,
            _value: ValueId,
            _field: u32,
            _span: TextRange,
        ) -> Result<ValueId, Vec<BackendError>> {
            Ok(ValueId(0))
        }
    }

    #[test]
    fn unsigned_narrow_parameter_only_masks() {
        let mut recorder = Recorder::default();
        let _ = narrow_integer(
            &mut recorder,
            ValueId(0),
            8,
            false,
            BlockId(0),
            TextRange::new(0, 1),
        )
        .expect("masking an unsigned narrow integer");
        assert_eq!(recorder.primitives, vec![NumericOp::I32And]);
    }

    #[test]
    fn signed_narrow_parameter_masks_and_sign_extends() {
        let mut recorder = Recorder::default();
        let _ = narrow_integer(
            &mut recorder,
            ValueId(0),
            16,
            true,
            BlockId(0),
            TextRange::new(0, 1),
        )
        .expect("masking and sign-extending a signed narrow integer");
        assert_eq!(
            recorder.primitives,
            vec![NumericOp::I32And, NumericOp::I32Shl, NumericOp::I32ShrS]
        );
    }
}
