//! Record, flags, and string element copies for the generic list copy.

use super::super::helpers::ValueOps;
use super::super::wasm_error;
use super::{Structurer, load_scalar, store_scalar};
use crate::BackendError;
use crate::abi;
use crate::cc::GuestLayout;
use crate::types::ValueId;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

impl Structurer<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_record_store(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        struct_type: u32,
        pointer: ValueId,
        stride: i32,
        fields: &[crate::abi::canonical::CanonicalField],
        element_guest: &GuestLayout,
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let GuestLayout::Product { labels, .. } = element_guest else {
            return Err(wasm_error(
                span,
                "canonical record element has no product layout",
            ));
        };
        let layouts = abi::layout::record_fields(
            fields
                .iter()
                .map(|field| abi::layout::parameter_layout(&field.ty)),
        )
        .ok_or_else(|| wasm_error(span, "canonical list element has no record layout"))?;
        for (field, (offset, _)) in fields.iter().zip(&layouts) {
            let label = abi::source_field_name(&field.name);
            let index = label_index(labels, &label, span)?;
            if field.ty.is_byte_list() {
                self.emit_string_store(
                    body,
                    array,
                    array_type,
                    pointer,
                    index_local,
                    scratch_local,
                    *offset,
                    Some(index),
                    struct_type,
                    stride,
                    span,
                )?;
            } else {
                self.element_address(body, pointer, index_local, stride, span)?;
                self.array_get(body, array, array_type, index_local, span)?;
                body.push(Op::Leaf(Instruction::StructGet {
                    struct_type_index: struct_type,
                    field_index: index,
                }));
                store_scalar(body, &field.ty, *offset, span)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_record_load(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        struct_type: u32,
        pointer: ValueId,
        stride: i32,
        fields: &[crate::abi::canonical::CanonicalField],
        element_guest: &GuestLayout,
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let GuestLayout::Product { labels, .. } = element_guest else {
            return Err(wasm_error(
                span,
                "canonical record element has no product layout",
            ));
        };
        let layouts = abi::layout::record_fields(
            fields
                .iter()
                .map(|field| abi::layout::parameter_layout(&field.ty)),
        )
        .ok_or_else(|| wasm_error(span, "canonical list element has no record layout"))?;
        self.element_address(body, pointer, index_local, stride, span)?;
        body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        self.load(body, array, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        for (field, (offset, _)) in fields.iter().zip(&layouts) {
            let label = abi::source_field_name(&field.name);
            let _ = label_index(labels, &label, span)?;
            if field.ty.is_byte_list() {
                body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                body.push(Op::Leaf(Instruction::I32Load(
                    super::super::ops::memory_with_align(*offset, 2),
                )));
                body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                body.push(Op::Leaf(Instruction::I32Load(
                    super::super::ops::memory_with_align(*offset + 4, 2),
                )));
                let decode = self.function_index(abi::BYTES_TO_STRING_SYMBOL, span)?;
                body.push(Op::Leaf(Instruction::Call(decode)));
                self.free_string_at(body, scratch_local, *offset, span)?;
            } else {
                body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                load_scalar(body, &field.ty, *offset, span)?;
            }
        }
        body.push(Op::Leaf(Instruction::StructNew(struct_type)));
        body.push(Op::Leaf(Instruction::ArraySet(array_type)));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_flags_store(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        struct_type: u32,
        pointer: ValueId,
        stride: i32,
        names: &[String],
        element_guest: &GuestLayout,
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let GuestLayout::Product { labels, .. } = element_guest else {
            return Err(wasm_error(
                span,
                "canonical flags element has no product layout",
            ));
        };
        if names.len() > 32 {
            return Err(wasm_error(span, "canonical flags list element is too wide"));
        }
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        for (index, label) in labels.iter().enumerate() {
            let bit = names
                .iter()
                .position(|name| abi::source_field_name(name) == *label)
                .ok_or_else(|| wasm_error(span, "canonical flags element has no field"))?;
            self.array_get(body, array, array_type, index_local, span)?;
            body.push(Op::Leaf(Instruction::StructGet {
                struct_type_index: struct_type,
                field_index: index as u32,
            }));
            if bit != 0 {
                body.push(Op::Leaf(Instruction::I32Const(bit as i32)));
                body.push(Op::Leaf(Instruction::I32Shl));
            }
            body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
            body.push(Op::Leaf(Instruction::I32Or));
            body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        }
        self.element_address(body, pointer, index_local, stride, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
        body.push(Op::Leaf(match flags_width(names.len()) {
            1 => Instruction::I32Store8(super::super::ops::memory_with_align(0, 0)),
            2 => Instruction::I32Store16(super::super::ops::memory_with_align(0, 1)),
            _ => Instruction::I32Store(super::super::ops::memory(0)),
        }));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_flags_load(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        struct_type: u32,
        pointer: ValueId,
        stride: i32,
        names: &[String],
        element_guest: &GuestLayout,
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let GuestLayout::Product { labels, .. } = element_guest else {
            return Err(wasm_error(
                span,
                "canonical flags element has no product layout",
            ));
        };
        if names.len() > 32 {
            return Err(wasm_error(span, "canonical flags list element is too wide"));
        }
        self.element_address(body, pointer, index_local, stride, span)?;
        body.push(Op::Leaf(match flags_width(names.len()) {
            1 => Instruction::I32Load8U(super::super::ops::memory_with_align(0, 0)),
            2 => Instruction::I32Load16U(super::super::ops::memory_with_align(0, 1)),
            _ => Instruction::I32Load(super::super::ops::memory(0)),
        }));
        body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        self.load(body, array, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        for label in labels {
            let bit = names
                .iter()
                .position(|name| abi::source_field_name(name) == *label)
                .ok_or_else(|| wasm_error(span, "canonical flags element has no field"))?;
            body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
            if bit != 0 {
                body.push(Op::Leaf(Instruction::I32Const(bit as i32)));
                body.push(Op::Leaf(Instruction::I32ShrU));
            }
            body.push(Op::Leaf(Instruction::I32Const(1)));
            body.push(Op::Leaf(Instruction::I32And));
        }
        body.push(Op::Leaf(Instruction::StructNew(struct_type)));
        body.push(Op::Leaf(Instruction::ArraySet(array_type)));
        Ok(())
    }

    /// Transcodes a GC string into the canonical `(pointer, length)` at `offset`
    /// within the element. `field` selects a struct field, or `None` for a
    /// string list element.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_string_store(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        pointer: ValueId,
        index_local: u32,
        scratch_local: u32,
        offset: u32,
        field: Option<u32>,
        struct_type: u32,
        stride: i32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let encode = self.function_index(abi::STRING_TO_BYTES_SYMBOL, span)?;
        self.array_get(body, array, array_type, index_local, span)?;
        if let Some(field) = field {
            body.push(Op::Leaf(Instruction::StructGet {
                struct_type_index: struct_type,
                field_index: field,
            }));
        }
        body.push(Op::Leaf(Instruction::RefAsNonNull));
        body.push(Op::Leaf(Instruction::Call(encode)));
        body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        self.element_address(body, pointer, index_local, stride, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
        body.push(Op::Leaf(Instruction::I32Const(4)));
        body.push(Op::Leaf(Instruction::I32Add));
        body.push(Op::Leaf(Instruction::I32Store(
            super::super::ops::memory_with_align(offset, 2),
        )));
        self.element_address(body, pointer, index_local, stride, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(0, 2),
        )));
        body.push(Op::Leaf(Instruction::I32Store(
            super::super::ops::memory_with_align(offset + 4, 2),
        )));
        Ok(())
    }

    /// Decodes a string list element's `(pointer, length)` at `offset`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_string_load(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        pointer: ValueId,
        index_local: u32,
        scratch_local: u32,
        offset: u32,
        stride: i32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let decode = self.function_index(abi::BYTES_TO_STRING_SYMBOL, span)?;
        self.element_address(body, pointer, index_local, stride, span)?;
        body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        self.load(body, array, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(offset, 2),
        )));
        body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(offset + 4, 2),
        )));
        body.push(Op::Leaf(Instruction::Call(decode)));
        body.push(Op::Leaf(Instruction::ArraySet(array_type)));
        self.free_string_at(body, scratch_local, offset, span)
    }

    /// Frees the `(pointer, length)` at `offset` in the element base held in
    /// `scratch_local`.
    pub(super) fn free_string_at(
        &self,
        body: &mut Body,
        scratch_local: u32,
        offset: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let realloc = self.function_index(abi::REALLOC_SYMBOL, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(offset, 2),
        )));
        body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(offset + 4, 2),
        )));
        body.push(Op::Leaf(Instruction::I32Const(1)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::Call(realloc)));
        body.push(Op::Leaf(Instruction::Drop));
        Ok(())
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

/// The canonical byte width of a flags value with `count` names.
fn flags_width(count: usize) -> u32 {
    match count {
        1..=8 => 1,
        9..=16 => 2,
        _ => 4,
    }
}

fn label_index(labels: &[String], label: &str, span: TextRange) -> Result<u32, Vec<BackendError>> {
    labels
        .iter()
        .position(|candidate| candidate == label)
        .map(|index| index as u32)
        .ok_or_else(|| wasm_error(span, "canonical list element has no matching field"))
}
