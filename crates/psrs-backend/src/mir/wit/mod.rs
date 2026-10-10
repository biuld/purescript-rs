//! Canonical ABI adaptation for calls to WIT imports. This is the one place
//! that knows about the component boundary: declared arguments are flattened to
//! canonical parameters, a returned `list`/`string` is read from the return
//! pointer, and the scratch and `cabi_realloc` conventions live here. MIR
//! itself only carries the resulting calls and memory operations.
//!
//! See `docs/design/backend/wasm/canonical-abi-and-wit.md`.

mod aggregate;
mod bind;
mod call_lowerer;
mod free;
mod function_lowerer;
mod handles;
mod lists;
mod parameters;
mod plan;

pub(super) use bind::BoundFn;
pub(super) use call_lowerer::WitCallLowerer;
pub(super) use handles::verify_function;
pub(super) use parameters::lower_string;
pub(in crate::mir) use plan::CallPlan;

use super::{BlockId, instruction::Instruction};
use crate::BackendError;
use crate::abi::canonical::CanonicalType;
use crate::abi::{self, WasiImport};
use crate::cc::GuestLayout;
use crate::mir::UnaryOp;
use crate::types::{MemoryId, ValueId, ValueType};
use psrs_span::TextRange;

/// Whether a canonical result is read directly from a register rather than
/// through a return pointer: a scalar, handle, `Char`, `Bool`, or nullary enum.
fn is_direct_result(ty: &CanonicalType) -> bool {
    matches!(
        ty,
        CanonicalType::Bool
            | CanonicalType::Int { .. }
            | CanonicalType::Float { .. }
            | CanonicalType::Char
            | CanonicalType::Enum(_)
            | CanonicalType::Handle { .. }
    )
}

/// A call-local buffer that must be freed once the canonical call returns. The
/// `align` is a compile-time constant; `length` is the payload byte length.
pub(super) struct PendingFree {
    pub pointer: ValueId,
    pub length: ValueId,
    pub align: i32,
    /// Element payloads of a `list<T>` buffer that owns host buffers, such as
    /// `list<string>` or `list<record>` with string fields. `length` remains the
    /// whole-buffer byte size.
    pub elements: Option<ElementFree>,
}

/// The per-element frees of a call-local list buffer. The guest layout and
/// canonical element drive the element copy and free so a `list<record>` with
/// string fields frees each field, and a `list<string>` frees each string.
pub(super) struct ElementFree {
    pub count: ValueId,
    pub element: CanonicalType,
    pub element_guest: GuestLayout,
}

/// Lowers a call to a WIT import from the declared arguments and the import's
/// canonical signature. Declared scalars and resource handles map to one
/// canonical parameter; a 64-bit scalar is widened; a `String` argument maps to
/// the `(pointer, length)` of its buffer. A return pointer is passed when the
/// canonical result does not fit in one value.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    signature: &crate::cc::Signature,
    projection: Option<&crate::cc::ExternalProjection>,
    destination: ValueId,
    arguments: &[ValueId],
    span: TextRange,
    entry: BlockId,
) -> Result<BlockId, Vec<BackendError>> {
    // Canonical construction produces a concrete guest value. An erased
    // source slot is filled only after the shared payload protocol discharges
    // its concrete representation, including block parameter types.
    let concrete = projection
        .and_then(|projection| projection.result.as_ref())
        .map(GuestLayout::shape)
        .filter(|shape| {
            matches!(
                signature.result,
                crate::cc::ValueShape::Reference(crate::cc::Reference {
                    heap: crate::cc::RefShape::Erased | crate::cc::RefShape::Aggregate,
                    ..
                })
            ) && *shape != signature.result
        });
    let Some(shape) = concrete else {
        return lower_concrete(
            lowerer,
            import,
            signature,
            projection,
            destination,
            arguments,
            span,
            entry,
        );
    };
    let ty = lowerer.wit_value_type(shape).ok_or_else(|| {
        vec![BackendError::invalid_ir(
            "P9 canonical call planning",
            span,
            "canonical result has no checked concrete value type",
        )]
    })?;
    let value = lowerer.fresh_wit_value(ty);
    let current = lower_concrete(
        lowerer, import, signature, projection, value, arguments, span, entry,
    )?;
    if signature.result == crate::cc::payload::erased_shape() {
        let (current, value) = lowerer.wit_payload_conversion(current, value, shape, true, span)?;
        lowerer.append_wit_instruction(
            current,
            Instruction::Copy {
                destination,
                value,
                span,
            },
            span,
        )?;
        return Ok(current);
    }
    let Some(ValueType::Ref(reference)) = lowerer.wit_value_type(signature.result) else {
        return Err(vec![BackendError::invalid_ir(
            "P9 canonical call planning",
            span,
            "canonical aggregate result has no checked reference slot",
        )]);
    };
    lowerer.append_wit_instruction(
        current,
        Instruction::RefCast {
            destination,
            value,
            reference,
            span,
        },
        span,
    )?;
    Ok(current)
}

#[allow(clippy::too_many_arguments)]
fn lower_concrete<L: WitCallLowerer>(
    lowerer: &mut L,
    import: &WasiImport,
    signature: &crate::cc::Signature,
    projection: Option<&crate::cc::ExternalProjection>,
    destination: ValueId,
    arguments: &[ValueId],
    span: TextRange,
    entry: BlockId,
) -> Result<BlockId, Vec<BackendError>> {
    let bound = BoundFn::bind(import, signature, projection);
    let mut flat = Vec::new();
    let mut frees = Vec::new();
    let mut current = parameters::lower_parameters(
        lowerer, import, &bound, arguments, &mut flat, &mut frees, entry, span,
    )?;
    let mut retptr = None;
    if import.abi.retptr {
        let pointer =
            aggregate::retptr_buffer(lowerer, import.abi.result_area, &mut frees, current, span)?;
        flat.push(pointer);
        retptr = Some(pointer);
    }
    match &import.canonical_result {
        // A returned list or string is written through the return pointer as
        // `(pointer, length)` of UTF-8 bytes. Decode it into a fresh GC string.
        Some(ty) if ty.is_byte_list() => {
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
        Some(CanonicalType::List(element)) => {
            let shape = bound
                .result
                .as_ref()
                .and_then(|result| result.guest_layout(lowerer));
            lists::read_value_list_result(
                lowerer,
                import,
                element,
                shape.as_ref(),
                destination,
                flat,
                retptr,
                current,
                span,
            )?;
        }
        Some(CanonicalType::FixedList { element, length }) => {
            let shape = bound
                .result
                .as_ref()
                .and_then(|result| result.guest_layout(lowerer));
            lists::read_fixed_list_result(
                lowerer,
                import,
                element,
                *length,
                shape.as_ref(),
                destination,
                flat,
                retptr,
                current,
                span,
            )?;
        }
        Some(ty) if is_direct_result(ty) => match import.result {
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
        None => {
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
        Some(ty) if abi::canonical::is_variant(ty) => {
            let result = bound
                .result
                .as_ref()
                .expect("a variant result has a bound guest shape");
            let guest = result
                .guest_layout(lowerer)
                .ok_or_else(|| aggregate::unsupported(span))?;
            current = aggregate::lower_variant_result(
                lowerer,
                import,
                &guest,
                destination,
                flat,
                retptr,
                current,
                span,
            )?;
        }
        Some(ty @ CanonicalType::Record(_)) => {
            let result = bound
                .result
                .as_ref()
                .expect("a record result has a bound guest shape");
            let guest = result
                .guest_layout(lowerer)
                .ok_or_else(|| aggregate::unsupported(span))?;
            current = aggregate::lower_record_result(
                lowerer,
                import,
                ty,
                &guest,
                destination,
                flat,
                retptr,
                current,
                span,
            )?;
        }
        Some(_) => {
            // The ABI surface check reports an unsupported shape before MIR
            // lowering, so an unmodeled result here is invalid compiler IR.
            return Err(vec![BackendError::invalid_ir(
                "P9 MIR lowering",
                span,
                "aggregate WIT results must be rejected before MIR lowering",
            )]);
        }
    }
    // Call-local buffers (string transcode buffers, list buffers, and indirect
    // parameter records) are owned by this function and freed once the call
    // returns. Each buffer's element payloads are freed first.
    for pending in frees.iter().rev() {
        if let Some(elements) = &pending.elements {
            lists::free_elements(lowerer, pending.pointer, elements, current, span)?;
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
    // The compiler does not drop or release a handle on its own: the standard
    // library owns the lifetime discipline and calls `resource.drop` explicitly
    // (DEC-14).
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
