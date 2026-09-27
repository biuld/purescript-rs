//! Canonical ABI adaptation for calls to WIT imports. This is the one place
//! that knows about the component boundary: declared arguments are flattened to
//! canonical parameters, a returned `list`/`string` is read from the return
//! pointer, and the scratch and `cabi_realloc` conventions live here. MIR
//! itself only carries the resulting calls and memory operations.
//!
//! See `docs/design/backend/wasm/canonical-abi-and-wit.md`.

mod aggregate;
mod call_lowerer;
mod function_lowerer;
mod handles;
mod lists;
mod parameters;

pub(super) use call_lowerer::WitCallLowerer;
pub(super) use handles::{OwnedObligation, owned_drops, verify_function};

use super::{BlockId, instruction::Instruction};
use crate::BackendError;
use crate::abi::{self, WasiImport};
use crate::mir::{NumericOp, UnaryOp};
use crate::types::{MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

/// A call-local buffer that must be freed once the canonical call returns. The
/// `align` is a compile-time constant; `length` is the payload length value.
pub(super) struct PendingFree {
    pub pointer: ValueId,
    pub length: ValueId,
    pub align: i32,
    /// Payloads of a `list<string>` or a `list<record>` with string fields that
    /// must be freed after the host has copied them. `length` remains the byte
    /// size.
    pub string_elements: Option<StringFree>,
}

/// The string payloads a call-local list buffer owns.
pub(super) enum StringFree {
    /// One string per element; the value is the element count.
    Scalars(ValueId),
    /// Records with string fields; the value is the element count, `size` is the
    /// element byte stride, and the plan locates each string field.
    Records {
        count: ValueId,
        size: u32,
        fields: Vec<crate::mir::ListFieldCopy>,
    },
}

/// Lowers a call to a WIT import from the declared arguments and the import's
/// canonical signature. Declared scalars and resource handles map to one
/// canonical parameter; a 64-bit scalar is widened; a `String` argument maps to
/// the `(pointer, length)` of its length-prefixed buffer. A return pointer is
/// passed when the canonical result does not fit in one value.
#[cfg(test)]
pub(super) fn lower<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    signature: &crate::cc::Signature,
    destination: ValueId,
    arguments: &[ValueId],
    span: TextRange,
    entry: BlockId,
) -> Result<BlockId, Vec<BackendError>> {
    lower_with_payloads(
        lowerer,
        import,
        signature,
        &crate::cc::ExternalPayloads::default(),
        destination,
        arguments,
        span,
        entry,
    )
}

/// Lowers a call with the external's concrete payload tree, so a mapped
/// aggregate payload that is itself a record, list, or variant can be laid out.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_with_payloads<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    signature: &crate::cc::Signature,
    payloads: &crate::cc::ExternalPayloads,
    destination: ValueId,
    arguments: &[ValueId],
    span: TextRange,
    entry: BlockId,
) -> Result<BlockId, Vec<BackendError>> {
    let mut flat = Vec::new();
    let mut frees = Vec::new();
    let mut current = parameters::lower_parameters(
        lowerer, import, signature, payloads, arguments, &mut flat, &mut frees, entry, span,
    )?;
    let mut retptr = None;
    if import.retptr {
        let pointer =
            aggregate::retptr_buffer(lowerer, &import.result_kind, &mut frees, current, span)?;
        flat.push(pointer);
        retptr = Some(pointer);
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
        abi::WasiResultKind::Option { .. }
        | abi::WasiResultKind::ValueResult { .. }
        | abi::WasiResultKind::Variant { .. } => {
            let result_node = match &payloads.result {
                crate::cc::PayloadNode::None => None,
                node => Some(node),
            };
            current = aggregate::lower_variant_result(
                lowerer,
                import,
                &signature.result,
                result_node,
                destination,
                flat,
                retptr,
                current,
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
        match &pending.string_elements {
            Some(StringFree::Scalars(count)) => {
                lists::free_string_elements(lowerer, pending.pointer, *count, current, span)?;
            }
            Some(StringFree::Records {
                count,
                size,
                fields,
            }) => {
                lists::free_record_string_elements(
                    lowerer,
                    pending.pointer,
                    *count,
                    *size,
                    fields,
                    current,
                    span,
                )?;
            }
            None => {}
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
    Ok(current)
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
