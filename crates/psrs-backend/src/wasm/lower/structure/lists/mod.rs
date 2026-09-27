//! Wasm loops for canonical non-byte lists. The MIR instruction stays
//! straight-line so the extent checker can see allocator provenance without
//! following a dynamic index. One parameterized instruction covers scalars,
//! strings, records, and flags, selected by the canonical element and its guest
//! layout.

mod element;

use super::super::wasm_error;
use super::Structurer;
use super::helpers::ValueOps;
use crate::BackendError;
use crate::abi;
use crate::abi::canonical::CanonicalType;
use crate::abi::layout::SlotKind;
use crate::mir::{Instruction as MirInstruction, ListDirection};
use crate::types::ValueId;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

impl Structurer<'_> {
    pub(super) fn emit_list_copy(
        &self,
        instruction: &MirInstruction,
    ) -> Result<Body, Vec<BackendError>> {
        let MirInstruction::ListCopy {
            direction,
            array,
            array_type,
            struct_type,
            pointer,
            length,
            element,
            element_guest,
            span,
        } = instruction
        else {
            unreachable!("list copy received another instruction")
        };
        let Some((index_local, scratch_local)) = self.list_locals else {
            return Err(wasm_error(*span, "canonical list copy has no loop locals"));
        };
        let stride = abi::canonical::size_align(element).size as i32;
        let mut body = Body::new();
        if *direction == ListDirection::Load {
            self.load(&mut body, *length, *span)?;
            body.push(Op::Leaf(Instruction::ArrayNewDefault(array_type.0)));
            self.store(&mut body, *array, *span)?;
        }
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::LocalSet(index_local)));
        let mut loop_body = Body::new();
        loop_body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        self.load(&mut loop_body, *length, *span)?;
        loop_body.push(Op::Leaf(Instruction::I32GeU));
        loop_body.push(Op::Leaf(Instruction::BrIf(1)));
        match direction {
            ListDirection::Store => self.emit_element_store(
                &mut loop_body,
                *array,
                array_type.0,
                struct_type.0,
                *pointer,
                stride,
                element,
                element_guest,
                index_local,
                scratch_local,
                *span,
            )?,
            ListDirection::Load => self.emit_element_load(
                &mut loop_body,
                *array,
                array_type.0,
                struct_type.0,
                *pointer,
                stride,
                element,
                element_guest,
                index_local,
                scratch_local,
                *span,
            )?,
            ListDirection::Free => self.emit_element_free(
                &mut loop_body,
                *pointer,
                stride,
                element,
                element_guest,
                index_local,
                scratch_local,
                *span,
            )?,
        }
        loop_body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        loop_body.push(Op::Leaf(Instruction::I32Const(1)));
        loop_body.push(Op::Leaf(Instruction::I32Add));
        loop_body.push(Op::Leaf(Instruction::LocalSet(index_local)));
        loop_body.push(Op::Leaf(Instruction::Br(0)));
        body.push(Op::Block {
            body: vec![Op::Loop {
                body: loop_body,
                result: None,
                span: *span,
            }],
            result: None,
            span: *span,
        });
        Ok(body)
    }

    /// Emits one element store. A byte-list element and a record's byte-list
    /// field transcode a GC string; a scalar element and a record's scalar
    /// field store a canonical slot.
    #[allow(clippy::too_many_arguments)]
    fn emit_element_store(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        struct_type: u32,
        pointer: ValueId,
        stride: i32,
        element: &CanonicalType,
        element_guest: &crate::cc::GuestLayout,
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if element.is_byte_list() {
            return self.emit_string_store(
                body,
                array,
                array_type,
                pointer,
                index_local,
                scratch_local,
                0,
                None,
                struct_type,
                stride,
                span,
            );
        }
        match element {
            CanonicalType::Record(fields) => self.emit_record_store(
                body,
                array,
                array_type,
                struct_type,
                pointer,
                stride,
                fields,
                element_guest,
                index_local,
                scratch_local,
                span,
            ),
            CanonicalType::Flags(names) => self.emit_flags_store(
                body,
                array,
                array_type,
                struct_type,
                pointer,
                stride,
                names,
                element_guest,
                index_local,
                scratch_local,
                span,
            ),
            _ => {
                self.element_address(body, pointer, index_local, stride, span)?;
                self.array_get(body, array, array_type, index_local, span)?;
                store_scalar(body, element, 0, span)?;
                Ok(())
            }
        }
    }

    /// Emits one element load.
    #[allow(clippy::too_many_arguments)]
    fn emit_element_load(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        struct_type: u32,
        pointer: ValueId,
        stride: i32,
        element: &CanonicalType,
        element_guest: &crate::cc::GuestLayout,
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if element.is_byte_list() {
            return self.emit_string_load(
                body,
                array,
                array_type,
                pointer,
                index_local,
                scratch_local,
                0,
                stride,
                span,
            );
        }
        match element {
            CanonicalType::Record(fields) => self.emit_record_load(
                body,
                array,
                array_type,
                struct_type,
                pointer,
                stride,
                fields,
                element_guest,
                index_local,
                scratch_local,
                span,
            ),
            CanonicalType::Flags(names) => self.emit_flags_load(
                body,
                array,
                array_type,
                struct_type,
                pointer,
                stride,
                names,
                element_guest,
                index_local,
                scratch_local,
                span,
            ),
            _ => {
                self.load(body, array, span)?;
                body.push(Op::Leaf(Instruction::LocalGet(index_local)));
                self.element_address(body, pointer, index_local, stride, span)?;
                load_scalar(body, element, 0, span)?;
                body.push(Op::Leaf(Instruction::ArraySet(array_type)));
                Ok(())
            }
        }
    }

    /// Emits the frees of one element's own buffers: a byte-list element's
    /// `(pointer, length)` and a record's byte-list fields.
    #[allow(clippy::too_many_arguments)]
    fn emit_element_free(
        &self,
        body: &mut Body,
        pointer: ValueId,
        stride: i32,
        element: &CanonicalType,
        _element_guest: &crate::cc::GuestLayout,
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if element.is_byte_list() {
            self.element_address(body, pointer, index_local, stride, span)?;
            body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
            return self.free_string_at(body, scratch_local, 0, span);
        }
        if let CanonicalType::Record(fields) = element {
            let layouts = abi::layout::record_fields(
                fields
                    .iter()
                    .map(|field| abi::layout::parameter_layout(&field.ty)),
            )
            .ok_or_else(|| wasm_error(span, "canonical list element has no record layout"))?;
            self.element_address(body, pointer, index_local, stride, span)?;
            body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
            for (field, (offset, _)) in fields.iter().zip(&layouts) {
                if field.ty.is_byte_list() {
                    self.free_string_at(body, scratch_local, *offset, span)?;
                }
            }
        }
        Ok(())
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

/// Stores a source scalar value at `offset`, widening an `i64` or demoting an
/// `f32` to its canonical width.
fn store_scalar(
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
                super::ops::memory_with_align(offset, 3),
            )));
        }
        CanonicalType::Float { width: 32 } => {
            body.push(Op::Leaf(Instruction::F32DemoteF64));
            body.push(Op::Leaf(Instruction::F32Store(
                super::ops::memory_with_align(offset, 2),
            )));
        }
        _ => {
            let kind = scalar_kind(ty, span)?;
            store_slot_kind(body, kind, offset);
        }
    }
    Ok(())
}

/// Loads a source scalar value at `offset`, narrowing an `i64` or promoting an
/// `f32` to the GC representation.
fn load_scalar(
    body: &mut Body,
    ty: &CanonicalType,
    offset: u32,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    match ty {
        CanonicalType::Int { width: 64, .. } => {
            body.push(Op::Leaf(Instruction::I64Load(
                super::ops::memory_with_align(offset, 3),
            )));
            body.push(Op::Leaf(Instruction::I32WrapI64));
        }
        CanonicalType::Float { width: 32 } => {
            body.push(Op::Leaf(Instruction::F32Load(
                super::ops::memory_with_align(offset, 2),
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

impl Structurer<'_> {
    fn element_address(
        &self,
        body: &mut Body,
        pointer: ValueId,
        index_local: u32,
        size: i32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, pointer, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        body.push(Op::Leaf(Instruction::I32Const(size)));
        body.push(Op::Leaf(Instruction::I32Mul));
        body.push(Op::Leaf(Instruction::I32Add));
        Ok(())
    }

    fn array_get(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        index_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, array, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        body.push(Op::Leaf(Instruction::ArrayGet(array_type)));
        Ok(())
    }
}

fn store_slot_kind(body: &mut Body, kind: SlotKind, offset: u32) {
    let mem = super::ops::memory_with_align;
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
    let mem = super::ops::memory_with_align;
    match kind {
        SlotKind::Byte => body.push(Op::Leaf(Instruction::I32Load8U(mem(offset, 0)))),
        SlotKind::Half => body.push(Op::Leaf(Instruction::I32Load16U(mem(offset, 1)))),
        SlotKind::Word => body.push(Op::Leaf(Instruction::I32Load(mem(offset, 2)))),
        SlotKind::I64 => body.push(Op::Leaf(Instruction::I64Load(mem(offset, 3)))),
        SlotKind::F32 => body.push(Op::Leaf(Instruction::F32Load(mem(offset, 2)))),
        SlotKind::F64 => body.push(Op::Leaf(Instruction::F64Load(mem(offset, 3)))),
    }
}

pub(crate) fn uses_list_copy(function: &crate::mir::Function) -> bool {
    function.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, MirInstruction::ListCopy { .. }))
    })
}
