//! Leaf, record, flags, and string element copies for the generic list copy.
//!
//! Every function here reads or writes one canonical node at an offset within
//! the enclosing element, recursing through the same [`Structurer`] dispatch.

use super::super::wasm_error;
use super::{ListLoop, Projection, Structurer, is_erased};
use crate::BackendError;
use crate::abi;
use crate::abi::canonical::CanonicalType;
use crate::abi::layout::SlotKind;
use crate::cc::GuestLayout;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

impl Structurer<'_> {
    /// Stores one `list<u8>` element: a source `Array Int` value is
    /// range-checked to `0..255` before it is narrowed to one canonical byte
    /// ([DEC-16](../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
    pub(super) fn emit_byte_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        path: &[Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.element_address(body, context, offset, span)?;
        self.emit_project(body, context, path, span)?;
        let scratch = self.node_locals(context.depth).scratch_local;
        body.push(Op::Leaf(Instruction::LocalSet(scratch)));
        // Trap unless the value is a canonical byte.
        body.push(Op::Leaf(Instruction::LocalGet(scratch)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32LtS));
        body.push(Op::Leaf(Instruction::LocalGet(scratch)));
        body.push(Op::Leaf(Instruction::I32Const(255)));
        body.push(Op::Leaf(Instruction::I32GtS));
        body.push(Op::Leaf(Instruction::I32Or));
        body.push(Op::Leaf(Instruction::If(wasm_encoder::BlockType::Empty)));
        body.push(Op::Leaf(Instruction::Unreachable));
        body.push(Op::Leaf(Instruction::End));
        body.push(Op::Leaf(Instruction::LocalGet(scratch)));
        body.push(Op::Leaf(Instruction::I32Store8(
            super::super::ops::memory_with_align(0, 0),
        )));
        Ok(())
    }

    /// Stores a scalar node: push the canonical address, then the projected
    /// guest value, unboxing it when the guest field is erased.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_scalar_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        guest: &GuestLayout,
        path: &[Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.element_address(body, context, offset, span)?;
        self.emit_project(body, context, path, span)?;
        if let GuestLayout::Scalar { shape } = guest
            && is_erased(*shape)
        {
            self.emit_unbox(body, canonical, span)?;
        }
        store_scalar(body, canonical, 0, span)
    }

    /// Loads a scalar node and boxes it when the guest field is erased.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_scalar_load(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical: &CanonicalType,
        guest: &GuestLayout,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.element_address(body, context, offset, span)?;
        load_scalar(body, canonical, 0, span)?;
        if let GuestLayout::Scalar { shape } = guest
            && is_erased(*shape)
        {
            self.emit_box(body, canonical, span)?;
        }
        Ok(())
    }

    /// Unboxes an erased scalar field into its canonical value.
    pub(super) fn emit_unbox(
        &self,
        body: &mut Body,
        canonical: &CanonicalType,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let boxed = match canonical {
            CanonicalType::Float { width: 64 } => self.boxed_number(span)?,
            CanonicalType::Float { width: 32 } => self.boxed_number(span)?,
            CanonicalType::Bool
            | CanonicalType::Int { .. }
            | CanonicalType::Char
            | CanonicalType::Enum(_)
            | CanonicalType::Handle { .. } => self.boxed_integer(span)?,
            _ => {
                return Err(wasm_error(
                    span,
                    "canonical aggregate payload has no erased scalar lowering",
                ));
            }
        };
        body.push(Op::Leaf(wasm_encoder::Instruction::RefCastNonNull(
            crate::wasm::convert::heap_type(crate::types::HeapType::Index(
                crate::types::DefinedTypeId(boxed),
            )),
        )));
        body.push(Op::Leaf(Instruction::StructGet {
            struct_type_index: boxed,
            field_index: 0,
        }));
        Ok(())
    }

    /// Boxes a canonical scalar into an erased field.
    pub(super) fn emit_box(
        &self,
        body: &mut Body,
        canonical: &CanonicalType,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let boxed = match canonical {
            CanonicalType::Float { .. } => self.boxed_number(span)?,
            CanonicalType::Bool
            | CanonicalType::Int { .. }
            | CanonicalType::Char
            | CanonicalType::Enum(_)
            | CanonicalType::Handle { .. } => self.boxed_integer(span)?,
            _ => {
                return Err(wasm_error(
                    span,
                    "canonical aggregate payload has no erased scalar lowering",
                ));
            }
        };
        body.push(Op::Leaf(Instruction::StructNew(boxed)));
        Ok(())
    }

    /// Stores a `string`/byte-list node through the UTF-8 codec.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_string_node_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        guest: &GuestLayout,
        path: &[Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let shape = match guest {
            GuestLayout::Scalar { shape } => *shape,
            _ => return Err(wasm_error(span, "canonical string element is not a scalar")),
        };
        self.emit_project(body, context, path, span)?;
        if is_erased(shape) {
            body.push(Op::Leaf(Instruction::RefCastNonNull(
                crate::wasm::convert::heap_type(crate::types::HeapType::Index(
                    crate::types::DefinedTypeId(self.string_index(span)?),
                )),
            )));
        }
        body.push(Op::Leaf(Instruction::RefAsNonNull));
        let encode = self.function_index(abi::STRING_TO_BYTES_SYMBOL, span)?;
        body.push(Op::Leaf(Instruction::Call(encode)));
        let scratch = self.scratch(context.depth);
        body.push(Op::Leaf(Instruction::LocalSet(scratch)));
        self.element_address(body, context, offset, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch)));
        body.push(Op::Leaf(Instruction::I32Const(4)));
        body.push(Op::Leaf(Instruction::I32Add));
        body.push(Op::Leaf(Instruction::I32Store(
            super::super::ops::memory_with_align(0, 2),
        )));
        self.element_address(body, context, offset + 4, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch)));
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(0, 2),
        )));
        body.push(Op::Leaf(Instruction::I32Store(
            super::super::ops::memory_with_align(0, 2),
        )));
        Ok(())
    }

    /// Loads a `string`/byte-list node and frees its host buffer.
    pub(super) fn emit_string_node_load(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.element_address(body, context, offset, span)?;
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(0, 2),
        )));
        self.element_address(body, context, offset + 4, span)?;
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(0, 2),
        )));
        let decode = self.function_index(abi::BYTES_TO_STRING_SYMBOL, span)?;
        body.push(Op::Leaf(Instruction::Call(decode)));
        self.free_bytes_at(body, context, offset, span)
    }

    /// Frees the `(pointer, length)` byte buffer at `offset`.
    pub(super) fn free_bytes_at(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let realloc = self.function_index(abi::REALLOC_SYMBOL, span)?;
        self.element_address(body, context, offset, span)?;
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(0, 2),
        )));
        self.element_address(body, context, offset + 4, span)?;
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(0, 2),
        )));
        body.push(Op::Leaf(Instruction::I32Const(1)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::Call(realloc)));
        body.push(Op::Leaf(Instruction::Drop));
        Ok(())
    }

    pub(super) fn scratch(&self, depth: u32) -> u32 {
        self.list_locals
            .expect("a list copy has loop locals")
            .scratch(depth)
    }

    fn function_index(
        &self,
        symbol: psrs_hir::SymbolId,
        span: TextRange,
    ) -> Result<u32, Vec<BackendError>> {
        self.function_indices
            .get(&symbol)
            .map(|index| index.0)
            .ok_or_else(|| wasm_error(span, "canonical list copy is missing a codec function"))
    }
}

/// The canonical scalar slot kind of a directly lowered element or field.
fn scalar_kind(ty: &CanonicalType, span: TextRange) -> Result<SlotKind, Vec<BackendError>> {
    let layout = abi::layout::parameter_layout(ty)
        .ok_or_else(|| wasm_error(span, "canonical list element has no scalar layout"))?;
    if layout.slots.len() != 1 {
        return Err(wasm_error(
            span,
            "canonical list element is not a single scalar slot",
        ));
    }
    Ok(layout.slots[0].kind)
}

/// Stores a source scalar value, widening an `i64` or demoting an `f32`.
pub(super) fn store_scalar(
    body: &mut Body,
    ty: &CanonicalType,
    offset: u32,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    match ty {
        CanonicalType::Int { width: 64, signed } => {
            body.push(Op::Leaf(if *signed {
                Instruction::I64ExtendI32S
            } else {
                Instruction::I64ExtendI32U
            }));
            body.push(Op::Leaf(Instruction::I64Store(
                super::super::ops::memory_with_align(offset, 3),
            )));
        }
        CanonicalType::Float { width: 32 } => {
            body.push(Op::Leaf(Instruction::F32DemoteF64));
            body.push(Op::Leaf(Instruction::F32Store(
                super::super::ops::memory_with_align(offset, 2),
            )));
        }
        _ => {
            let kind = scalar_kind(ty, span)?;
            store_slot_kind(body, kind, offset);
        }
    }
    Ok(())
}

/// Loads a source scalar value, narrowing an `i64` or promoting an `f32`.
pub(super) fn load_scalar(
    body: &mut Body,
    ty: &CanonicalType,
    offset: u32,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    match ty {
        CanonicalType::Int { width: 64, .. } => {
            body.push(Op::Leaf(Instruction::I64Load(
                super::super::ops::memory_with_align(offset, 3),
            )));
            body.push(Op::Leaf(Instruction::I32WrapI64));
        }
        CanonicalType::Float { width: 32 } => {
            body.push(Op::Leaf(Instruction::F32Load(
                super::super::ops::memory_with_align(offset, 2),
            )));
            body.push(Op::Leaf(Instruction::F64PromoteF32));
        }
        _ => {
            let kind = scalar_kind(ty, span)?;
            load_slot_kind(body, kind, offset);
        }
    }
    Ok(())
}

fn store_slot_kind(body: &mut Body, kind: SlotKind, offset: u32) {
    let mem = super::super::ops::memory_with_align;
    match kind {
        SlotKind::Byte => body.push(Op::Leaf(Instruction::I32Store8(mem(offset, 0)))),
        SlotKind::Half => body.push(Op::Leaf(Instruction::I32Store16(mem(offset, 1)))),
        SlotKind::Word => body.push(Op::Leaf(Instruction::I32Store(mem(offset, 2)))),
        SlotKind::I64 => body.push(Op::Leaf(Instruction::I64Store(mem(offset, 3)))),
        SlotKind::F32 => body.push(Op::Leaf(Instruction::F32Store(mem(offset, 2)))),
        SlotKind::F64 => body.push(Op::Leaf(Instruction::F64Store(mem(offset, 3)))),
    }
}

fn load_slot_kind(body: &mut Body, kind: SlotKind, offset: u32) {
    let mem = super::super::ops::memory_with_align;
    match kind {
        SlotKind::Byte => body.push(Op::Leaf(Instruction::I32Load8U(mem(offset, 0)))),
        SlotKind::Half => body.push(Op::Leaf(Instruction::I32Load16U(mem(offset, 1)))),
        SlotKind::Word => body.push(Op::Leaf(Instruction::I32Load(mem(offset, 2)))),
        SlotKind::I64 => body.push(Op::Leaf(Instruction::I64Load(mem(offset, 3)))),
        SlotKind::F32 => body.push(Op::Leaf(Instruction::F32Load(mem(offset, 2)))),
        SlotKind::F64 => body.push(Op::Leaf(Instruction::F64Load(mem(offset, 3)))),
    }
}
