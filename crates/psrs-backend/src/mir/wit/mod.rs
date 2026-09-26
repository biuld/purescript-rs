//! Canonical ABI adaptation for calls to WIT imports. This is the one place
//! that knows about the component boundary: declared arguments are flattened to
//! canonical parameters, a returned `list`/`string` is read from the return
//! pointer, and the scratch and `cabi_realloc` conventions live here. MIR
//! itself only carries the resulting calls and memory operations.
//!
//! See `docs/design/backend/wasm/canonical-abi-and-wit.md`.

mod parameters;

use super::lower::FunctionLowerer;
use super::{BlockId, instruction::Instruction};
use crate::BackendError;
use crate::abi::{self, WasiImport};
use crate::mir::{NumericOp, UnaryOp};
use crate::types::{MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

pub(super) trait WitCallLowerer {
    fn fresh_wit_value(&mut self, ty: ValueType) -> ValueId;
    fn append_wit_instruction(
        &mut self,
        block: BlockId,
        instruction: Instruction,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>>;
    fn wit_product_field(
        &mut self,
        block: BlockId,
        value: ValueId,
        field: u32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>>;
}

impl WitCallLowerer for FunctionLowerer<'_> {
    fn fresh_wit_value(&mut self, ty: ValueType) -> ValueId {
        self.fresh(ty)
    }

    fn append_wit_instruction(
        &mut self,
        block: BlockId,
        instruction: Instruction,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.append_instruction(block, instruction, span)
    }

    fn wit_product_field(
        &mut self,
        block: BlockId,
        value: ValueId,
        field: u32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        self.wit_product_field(block, value, field, span)
    }
}

/// Lowers a call to a WIT import from the declared arguments and the import's
/// canonical signature. Declared scalars and resource handles map to one
/// canonical parameter; a 64-bit scalar is widened; a `String` argument maps to
/// the `(pointer, length)` of its length-prefixed buffer. A return pointer is
/// passed when the canonical result does not fit in one value.
pub(super) fn lower<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    source_signature: &abi::SourceSignature,
    destination: ValueId,
    arguments: &[ValueId],
    span: TextRange,
    current: BlockId,
) -> Result<(), Vec<BackendError>> {
    let mut flat = Vec::new();
    parameters::lower_parameters(
        lowerer,
        import,
        source_signature,
        arguments,
        &mut flat,
        current,
        span,
    )?;
    let mut retptr = None;
    if import.retptr {
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
        flat.push(scratch);
        retptr = Some(scratch);
    }
    match &import.result_kind {
        // A returned list or string is written through the return pointer as
        // `(pointer, length)` of UTF-8 bytes. Decode it into a fresh GC string.
        abi::WasiResultKind::List => {
            let address = retptr.expect("a list result takes a return pointer");
            lowerer.append_wit_instruction(
                current,
                Instruction::CallVoid {
                    function: import.symbol,
                    arguments: flat,
                    span,
                },
                span,
            )?;
            let pointer = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::Load {
                    destination: pointer,
                    address,
                    memory: MemoryId(0),
                    offset: 0,
                    span,
                },
                span,
            )?;
            let length = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::Load {
                    destination: length,
                    address,
                    memory: MemoryId(0),
                    offset: 4,
                    span,
                },
                span,
            )?;
            lowerer.append_wit_instruction(
                current,
                Instruction::Call {
                    destination,
                    function: crate::abi::BYTES_TO_STRING_SYMBOL,
                    arguments: vec![pointer, length],
                    span,
                },
                span,
            )?;
        }
        abi::WasiResultKind::Scalar
        | abi::WasiResultKind::IntegerNarrow { .. }
        | abi::WasiResultKind::Boolean
        | abi::WasiResultKind::Enum { .. }
        | abi::WasiResultKind::Char => match import.result {
            Some(ValueType::I64) => {
                let value = lowerer.fresh_wit_value(ValueType::I64);
                lowerer.append_wit_instruction(
                    current,
                    Instruction::Call {
                        destination: value,
                        function: import.symbol,
                        arguments: flat,
                        span,
                    },
                    span,
                )?;
                lowerer.append_wit_instruction(
                    current,
                    Instruction::WrapI64 {
                        destination,
                        value,
                        span,
                    },
                    span,
                )?;
            }
            Some(ValueType::F32) => {
                let value = lowerer.fresh_wit_value(ValueType::F32);
                lowerer.append_wit_instruction(
                    current,
                    Instruction::Call {
                        destination: value,
                        function: import.symbol,
                        arguments: flat,
                        span,
                    },
                    span,
                )?;
                lowerer.append_wit_instruction(
                    current,
                    Instruction::UnaryPrimitive {
                        destination,
                        op: UnaryOp::F32ToF64,
                        value,
                        span,
                    },
                    span,
                )?;
            }
            Some(ValueType::I32 | ValueType::F64) => lowerer.append_wit_instruction(
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
                // Classification rejects shapes with no canonical result
                // before MIR lowering; reaching here is invalid compiler IR.
                return Err(vec![BackendError::invalid_ir(
                    "P9 MIR lowering",
                    span,
                    "this WIT import's scalar result type is not supported yet",
                )]);
            }
        },
        abi::WasiResultKind::None => {
            lowerer.append_wit_instruction(
                current,
                Instruction::CallVoid {
                    function: import.symbol,
                    arguments: flat,
                    span,
                },
                span,
            )?;
            lowerer.append_wit_instruction(
                current,
                Instruction::Constant {
                    destination,
                    value: 0,
                    span,
                },
                span,
            )?;
        }
        abi::WasiResultKind::Result => {
            lowerer.append_wit_instruction(
                current,
                Instruction::CallVoid {
                    function: import.symbol,
                    arguments: flat,
                    span,
                },
                span,
            )?;
            let status = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::Load8U {
                    destination: status,
                    address: retptr.expect("a result takes a return pointer"),
                    memory: MemoryId(0),
                    offset: 0,
                    span,
                },
                span,
            )?;
            let zero = lowerer.fresh_wit_value(ValueType::I32);
            lowerer.append_wit_instruction(
                current,
                Instruction::Constant {
                    destination: zero,
                    value: 0,
                    span,
                },
                span,
            )?;
            let failed = lowerer.fresh_wit_value(ValueType::Boolean);
            lowerer.append_wit_instruction(
                current,
                Instruction::Primitive {
                    destination: failed,
                    op: NumericOp::I32Ne,
                    left: status,
                    right: zero,
                    span,
                },
                span,
            )?;
            lowerer.append_wit_instruction(
                current,
                Instruction::TrapIf {
                    condition: failed,
                    span,
                },
                span,
            )?;
            lowerer.append_wit_instruction(
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
            // The ABI classification reports an unsupported shape before MIR
            // lowering, so a `Discarded` result here is invalid compiler IR.
            return Err(vec![BackendError::invalid_ir(
                "P9 MIR lowering",
                span,
                "aggregate WIT results must be rejected before MIR lowering",
            )]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
