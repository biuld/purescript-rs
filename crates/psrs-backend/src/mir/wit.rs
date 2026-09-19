//! Canonical ABI adaptation for calls to WIT imports. This is the one place
//! that knows about the component boundary: declared arguments are flattened to
//! canonical parameters, a returned `list`/`string` is read from the return
//! pointer, and the scratch and `cabi_realloc` conventions live here. MIR
//! itself only carries the resulting calls and memory operations.
//!
//! See `docs/design/D-07-wit-imports-and-std.md`.

use super::lower::FunctionLowerer;
use super::{BlockId, instruction::Instruction};
use crate::BackendError;
use crate::abi::{self, WasiImport};
use crate::types::{ValueId, ValueType};
use psrs_core::Primitive;
use psrs_span::TextRange;

/// Lowers a call to a WIT import from the declared arguments and the import's
/// canonical signature. Declared scalars and resource handles map to one
/// canonical parameter; a 64-bit scalar is widened; a `String` argument maps to
/// the `(pointer, length)` of its length-prefixed buffer. A return pointer is
/// passed when the canonical result does not fit in one value.
pub(super) fn lower(
    lowerer: &mut FunctionLowerer<'_>,
    import: &WasiImport,
    destination: ValueId,
    arguments: &[ValueId],
    span: TextRange,
    current: BlockId,
) -> Result<(), Vec<BackendError>> {
    if import.param_kinds.len() != arguments.len() {
        return Err(vec![BackendError::new(
            "P9 MIR lowering",
            span,
            format!(
                "`{}` takes {} arguments, but {} were provided",
                import.name,
                import.param_kinds.len(),
                arguments.len()
            ),
        )]);
    }
    let mut flat = Vec::new();
    for (argument, kind) in arguments.iter().zip(&import.param_kinds) {
        match kind {
            abi::WasiParamKind::Scalar | abi::WasiParamKind::Handle => flat.push(*argument),
            abi::WasiParamKind::Scalar64 { signed } => {
                let wide = lowerer.fresh(ValueType::I64);
                lowerer.append_instruction(
                    current,
                    Instruction::WidenI64 {
                        destination: wide,
                        value: *argument,
                        signed: *signed,
                        span,
                    },
                    span,
                )?;
                flat.push(wide);
            }
            abi::WasiParamKind::List => {
                let length = lowerer.fresh(ValueType::I32);
                lowerer.append_instruction(
                    current,
                    Instruction::Load {
                        destination: length,
                        address: *argument,
                        offset: 0,
                        span,
                    },
                    span,
                )?;
                let four = lowerer.fresh(ValueType::I32);
                lowerer.append_instruction(
                    current,
                    Instruction::Constant {
                        destination: four,
                        value: 4,
                        span,
                    },
                    span,
                )?;
                let bytes = lowerer.fresh(ValueType::I32);
                lowerer.append_instruction(
                    current,
                    Instruction::Primitive {
                        destination: bytes,
                        op: Primitive::Add,
                        left: *argument,
                        right: four,
                        span,
                    },
                    span,
                )?;
                flat.push(bytes);
                flat.push(length);
            }
        }
    }
    let mut retptr = None;
    if import.retptr {
        let scratch = lowerer.fresh(ValueType::I32);
        lowerer.append_instruction(
            current,
            Instruction::Constant {
                destination: scratch,
                value: abi::PRINT_SCRATCH,
                span,
            },
            span,
        )?;
        flat.push(scratch);
        retptr = Some(scratch);
    }
    match import.result_kind {
        // A returned list or string is written through the return pointer as
        // `(pointer, length)`. `cabi_realloc` prefixes the buffer with its
        // length, so the string value is the pointer minus that prefix.
        abi::WasiResultKind::List => {
            let address = retptr.expect("a list result takes a return pointer");
            lowerer.append_instruction(
                current,
                Instruction::CallVoid {
                    function: import.symbol,
                    arguments: flat,
                    span,
                },
                span,
            )?;
            let pointer = lowerer.fresh(ValueType::I32);
            lowerer.append_instruction(
                current,
                Instruction::Load {
                    destination: pointer,
                    address,
                    offset: 0,
                    span,
                },
                span,
            )?;
            let four = lowerer.fresh(ValueType::I32);
            lowerer.append_instruction(
                current,
                Instruction::Constant {
                    destination: four,
                    value: 4,
                    span,
                },
                span,
            )?;
            lowerer.append_instruction(
                current,
                Instruction::Primitive {
                    destination,
                    op: Primitive::Sub,
                    left: pointer,
                    right: four,
                    span,
                },
                span,
            )?;
        }
        abi::WasiResultKind::Scalar => match import.result {
            Some(ValueType::I64) => {
                let value = lowerer.fresh(ValueType::I64);
                lowerer.append_instruction(
                    current,
                    Instruction::Call {
                        destination: value,
                        function: import.symbol,
                        arguments: flat,
                        span,
                    },
                    span,
                )?;
                lowerer.append_instruction(
                    current,
                    Instruction::WrapI64 {
                        destination,
                        value,
                        span,
                    },
                    span,
                )?;
            }
            Some(ValueType::I32) => lowerer.append_instruction(
                current,
                Instruction::Call {
                    destination,
                    function: import.symbol,
                    arguments: flat,
                    span,
                },
                span,
            )?,
            _ => {
                return Err(vec![BackendError::new(
                    "P9 MIR lowering",
                    span,
                    "this WIT import's scalar result type is not supported yet",
                )]);
            }
        },
        abi::WasiResultKind::None | abi::WasiResultKind::Result => {
            lowerer.append_instruction(
                current,
                Instruction::CallVoid {
                    function: import.symbol,
                    arguments: flat,
                    span,
                },
                span,
            )?;
            lowerer.append_instruction(
                current,
                Instruction::Constant {
                    destination,
                    value: 0,
                    span,
                },
                span,
            )?;
        }
        abi::WasiResultKind::Discarded => {
            return Err(vec![BackendError::new(
                "P9 MIR lowering",
                span,
                "aggregate WIT results must be rejected before MIR lowering",
            )]);
        }
    }
    Ok(())
}
