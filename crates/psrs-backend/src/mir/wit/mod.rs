//! Canonical ABI adaptation for calls to WIT imports. This is the one place
//! that knows about the component boundary: declared arguments are flattened to
//! canonical parameters, a returned `list`/`string` is read from the return
//! pointer, and the scratch and `cabi_realloc` conventions live here. MIR
//! itself only carries the resulting calls and memory operations.
//!
//! See `docs/design/backend/wasm/canonical-abi-and-wit.md`.

mod function_lowerer;
mod handles;
mod lists;
mod parameters;

pub(super) use handles::{OwnedObligation, owned_drops, verify_function};

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

    /// The record product fields and their canonical labels for a representation
    /// handle. `None` when the handle is not a product. The WIT adapter uses the
    /// labels to project fields by WIT name without a source-type mirror.
    fn wit_product(
        &self,
        _repr: crate::cc::ReprId,
    ) -> Option<(Vec<crate::cc::ValueShape>, Vec<String>)> {
        None
    }

    /// The element shape of a GC array representation handle.
    fn wit_array_element(&self, _repr: crate::cc::ReprId) -> Option<crate::cc::ValueShape> {
        None
    }

    /// The concrete GC type of a representation handle.
    fn wit_repr_index(&self, _repr: crate::cc::ReprId) -> Option<crate::types::DefinedTypeId> {
        None
    }

    /// Records an `own<T>` result that this function must drop unless it
    /// returns the index or passes it to another `own` parameter.
    fn note_owned(&mut self, _value: ValueId, _drop_symbol: psrs_hir::SymbolId, _span: TextRange) {}

    /// An `own<T>` argument consumes a previously noted handle.
    fn transfer_owned(&mut self, _value: ValueId) {}

    /// The GC array type of a source array value, when this lowerer has layouts.
    fn wit_array_type(
        &self,
        _value: ValueId,
        span: TextRange,
    ) -> Result<crate::types::DefinedTypeId, Vec<BackendError>> {
        Err(vec![BackendError::new(
            "P9 MIR lowering",
            span,
            "canonical list lowering has no GC array type",
        )])
    }
}

/// A call-local buffer that must be freed once the canonical call returns. The
/// `align` is a compile-time constant; `length` is the payload length value.
pub(super) struct PendingFree {
    pub pointer: ValueId,
    pub length: ValueId,
    pub align: i32,
    /// Element count of a `list<string>` parameter whose payloads must be freed
    /// after the host has copied them. `length` remains the byte size.
    pub string_elements: Option<ValueId>,
}

/// Lowers a call to a WIT import from the declared arguments and the import's
/// canonical signature. Declared scalars and resource handles map to one
/// canonical parameter; a 64-bit scalar is widened; a `String` argument maps to
/// the `(pointer, length)` of its length-prefixed buffer. A return pointer is
/// passed when the canonical result does not fit in one value.
pub(super) fn lower<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    signature: &crate::cc::Signature,
    destination: ValueId,
    arguments: &[ValueId],
    span: TextRange,
    current: BlockId,
) -> Result<(), Vec<BackendError>> {
    let mut flat = Vec::new();
    let mut frees = Vec::new();
    parameters::lower_parameters(
        lowerer, import, signature, arguments, &mut flat, &mut frees, current, span,
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
            // The host-allocated import result is copied into the GC string;
            // free it before returning control to source.
            free_buffer(lowerer, pointer, length, 1, current, span)?;
        }
        abi::WasiResultKind::ValueList { element } => {
            lists::read_value_list_result(
                lowerer,
                import,
                element,
                &signature.result,
                destination,
                flat,
                retptr,
                current,
                span,
            )?;
        }
        abi::WasiResultKind::Scalar
        | abi::WasiResultKind::Handle(_)
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
    // Call-local buffers (string transcode buffers and indirect parameter
    // records) are owned by this function and freed once the call returns.
    for pending in frees.iter().rev() {
        if let Some(count) = pending.string_elements {
            lists::free_string_elements(lowerer, pending.pointer, count, current, span)?;
        }
        free_buffer(
            lowerer,
            pending.pointer,
            pending.length,
            pending.align,
            current,
            span,
        )?;
    }
    // A borrow result cannot outlive this call: release it before the caller
    // can use the index. An owned result stays live until it is transferred
    // or the function drops it.
    if let abi::WasiResultKind::Handle(handle) = &import.result_kind {
        match handle.mode {
            abi::HandleMode::Borrow => {
                lowerer.append_wit_instruction(
                    current,
                    handles::borrow_release(destination, handle.drop_symbol, span),
                    span,
                )?;
            }
            abi::HandleMode::Own => lowerer.note_owned(destination, handle.drop_symbol, span),
        }
    }
    Ok(())
}

/// Frees a transient canonical buffer through `cabi_realloc(ptr, len, align, 0)`.
pub(super) fn free_buffer<L: WitCallLowerer>(
    lowerer: &mut L,
    pointer: ValueId,
    length: ValueId,
    align: i32,
    current: BlockId,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let align_value = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Constant {
            destination: align_value,
            value: align,
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
    // `cabi_realloc` is declared with an `i32` result; the freed pointer is
    // discarded.
    let discarded = lowerer.fresh_wit_value(ValueType::I32);
    lowerer.append_wit_instruction(
        current,
        Instruction::Call {
            destination: discarded,
            function: abi::REALLOC_SYMBOL,
            arguments: vec![pointer, length, align_value, zero],
            span,
        },
        span,
    )
}

#[cfg(test)]
mod tests;
