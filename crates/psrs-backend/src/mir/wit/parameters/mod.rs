use super::super::BlockId;
use super::super::instruction::Instruction;
use super::{BoundFn, PendingFree, WitCallLowerer};
use crate::BackendError;
use crate::abi::canonical::CanonicalType;
use crate::abi::{self, WasiImport};
use crate::cc::GuestLayout;
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
    bound: &BoundFn,
    arguments: &[ValueId],
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    entry: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    if bound.guest_parameters.len() != arguments.len() {
        return Err(parameter_count(span));
    }
    let mut current = entry;
    let mut flattened = Vec::new();
    // A primitive import of an aggregate (`option<string>` as `Int -> String`)
    // has a different source arity than the WIT parameters. Flatten each
    // primitive in order. Nullary enums, closed records, and flags stay on the
    // zip path.
    if bound.guest_parameters.len() != import.params.len() {
        current = primitive::lower_primitive_parameters(
            lowerer,
            import,
            &bound.guest_parameters,
            arguments,
            &mut flattened,
            frees,
            current,
            span,
        )?;
        flat.extend(flattened);
        return Ok(current);
    }
    for (argument, parameter) in arguments.iter().zip(&bound.parameters) {
        let guest = parameter.guest_layout(lowerer);
        current = lower_parameter(
            lowerer,
            *argument,
            guest.as_ref(),
            &parameter.canonical,
            &mut flattened,
            frees,
            current,
            span,
        )?;
    }
    if import.has_indirect_parameters() {
        current = indirect::write_parameter_record(
            lowerer,
            &import.params,
            &flattened,
            flat,
            frees,
            current,
            span,
        )?;
    } else {
        flat.extend(flattened);
    }
    Ok(current)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_parameter<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    guest: Option<&GuestLayout>,
    ty: &CanonicalType,
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<BlockId, Vec<BackendError>> {
    match ty {
        CanonicalType::Int { width: 32, .. }
        | CanonicalType::Bool
        | CanonicalType::Char
        | CanonicalType::Float { width: 64 }
        | CanonicalType::Enum(_) => flat.push(argument),
        // A handle is one canonical `i32` index. The compiler does not track its
        // ownership; the standard library drops it explicitly (DEC-14).
        CanonicalType::Handle { .. } => flat.push(argument),
        CanonicalType::Float { width: 32 } => {
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
        CanonicalType::Int { width: 64, signed } => {
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
        CanonicalType::Int { width, signed } if *width == 8 || *width == 16 => {
            let narrowed =
                narrow::narrow_integer(lowerer, argument, *width, *signed, current, span)?;
            flat.push(narrowed);
        }
        CanonicalType::Flags(names) => {
            lower_flags(lowerer, argument, guest, names, flat, current, span)?;
        }
        // Every mapped `result` is an `Either`, including one with an absent
        // payload position; `lower_variant_parameter` zero-fills the nullary
        // case.
        CanonicalType::Option(_) | CanonicalType::Result { .. } | CanonicalType::Variant(_) => {
            let guest = guest.ok_or_else(|| unsupported_parameter(span))?;
            return super::aggregate::lower_variant_parameter(
                lowerer, argument, guest, ty, flat, frees, current, span,
            );
        }
        CanonicalType::String => {
            lower_string(lowerer, argument, flat, frees, current, span)?;
        }
        CanonicalType::List(inner) if inner.is_byte() => {
            lower_string(lowerer, argument, flat, frees, current, span)?;
        }
        CanonicalType::FixedList { element, .. } if element.is_byte() => {
            lower_string(lowerer, argument, flat, frees, current, span)?;
        }
        CanonicalType::List(element) => {
            super::lists::write_value_list(
                lowerer, argument, element, guest, flat, frees, current, span,
            )?;
        }
        CanonicalType::FixedList { element, length } => {
            super::lists::write_fixed_list(
                lowerer, argument, element, *length, guest, flat, frees, current, span,
            )?;
        }
        CanonicalType::Record(fields) => {
            let Some(GuestLayout::Product {
                labels,
                fields: product,
                ..
            }) = guest
            else {
                return Err(unsupported_parameter(span));
            };
            if fields.len() != labels.len() || fields.len() != product.len() {
                return Err(unsupported_parameter(span));
            }
            let mut current = current;
            for field in fields {
                let source_label = abi::source_field_name(&field.name);
                let Some(index) = labels.iter().position(|label| label == &source_label) else {
                    return Err(unsupported_parameter(span));
                };
                let value = lowerer.wit_product_field(current, argument, index as u32, span)?;
                current = lower_parameter(
                    lowerer,
                    value,
                    Some(&product[index].value),
                    &field.ty,
                    flat,
                    frees,
                    current,
                    span,
                )?;
            }
            return Ok(current);
        }
        CanonicalType::Int { .. } | CanonicalType::Float { .. } => {
            return Err(unsupported_parameter(span));
        }
    }
    Ok(current)
}

/// Transcodes a GC string's UTF-16 into a fresh UTF-8 linear buffer. The helper
/// returns the address of a length prefix; the canonical exchange passes the
/// payload pointer and byte length, and the buffer is freed after the call.
fn lower_string<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    flat: &mut Vec<ValueId>,
    frees: &mut Vec<PendingFree>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
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
    frees.push(PendingFree {
        pointer: bytes,
        length,
        align: 1,
        elements: None,
    });
    Ok(())
}

fn lower_flags<L: WitCallLowerer>(
    lowerer: &mut L,
    argument: ValueId,
    guest: Option<&GuestLayout>,
    names: &[String],
    flat: &mut Vec<ValueId>,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let Some(GuestLayout::Product { labels, .. }) = guest else {
        return Err(unsupported_parameter(span));
    };
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
