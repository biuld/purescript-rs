//! Record and flags element copies for the generic list copy.
//!
//! A record projects each field by canonical label; a flags value packs its
//! boolean fields into the canonical 1/2/4-byte word (or several `i32` words
//! when it has more than 32 flags).

use super::super::wasm_error;
use super::{ListLoop, Projection, Structurer};
use crate::BackendError;
use crate::abi;
use crate::cc::ValueShape;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

impl Structurer<'_> {
    /// Stores a record node field by field.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_record_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical_fields: &[crate::abi::canonical::CanonicalField],
        repr: crate::cc::ReprId,
        labels: &[String],
        shapes: &[ValueShape],
        path: &[Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let layouts = abi::layout::record_fields(
            canonical_fields
                .iter()
                .map(|field| abi::layout::parameter_layout(&field.ty)),
        )
        .ok_or_else(|| wasm_error(span, "canonical list element has no record layout"))?;
        let repr_index = self.repr_index(repr, span)?;
        for (field, (field_offset, _)) in canonical_fields.iter().zip(&layouts) {
            let label = abi::source_field_name(&field.name);
            let index = label_index(labels, &label, span)?;
            let guest = self.resolve_guest(shapes[index as usize], span)?;
            let mut nested = path.to_vec();
            nested.push(Projection::Field {
                ty: repr_index,
                field: index,
            });
            self.emit_store_node(
                body,
                context,
                offset + field_offset,
                &field.ty,
                &guest,
                &nested,
                span,
            )?;
        }
        Ok(())
    }

    /// Loads a record node, pushing each field in source order.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_record_load(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical_fields: &[crate::abi::canonical::CanonicalField],
        repr: crate::cc::ReprId,
        labels: &[String],
        shapes: &[ValueShape],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let layouts = abi::layout::record_fields(
            canonical_fields
                .iter()
                .map(|field| abi::layout::parameter_layout(&field.ty)),
        )
        .ok_or_else(|| wasm_error(span, "canonical list element has no record layout"))?;
        let repr_index = self.repr_index(repr, span)?;
        for (index, shape) in shapes.iter().enumerate() {
            let label = &labels[index];
            let canonical_index = canonical_fields
                .iter()
                .position(|field| &abi::source_field_name(&field.name) == label)
                .ok_or_else(|| wasm_error(span, "canonical record element has no field"))?;
            let field_offset = layouts[canonical_index].0;
            let guest = self.resolve_guest(*shape, span)?;
            self.emit_load_node(
                body,
                context,
                offset + field_offset,
                &canonical_fields[canonical_index].ty,
                &guest,
                span,
            )?;
        }
        body.push(Op::Leaf(Instruction::StructNew(repr_index)));
        Ok(())
    }

    /// Frees the nested buffers of a record node.
    pub(super) fn emit_record_free(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        canonical_fields: &[crate::abi::canonical::CanonicalField],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let layouts = abi::layout::record_fields(
            canonical_fields
                .iter()
                .map(|field| abi::layout::parameter_layout(&field.ty)),
        )
        .ok_or_else(|| wasm_error(span, "canonical list element has no record layout"))?;
        for (field, (field_offset, _)) in canonical_fields.iter().zip(&layouts) {
            self.emit_free_node(body, context, offset + field_offset, &field.ty, span)?;
        }
        Ok(())
    }

    /// Packs a flags node into its canonical words.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_flags_store(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        names: &[String],
        repr: crate::cc::ReprId,
        labels: &[String],
        path: &[Projection],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let repr_index = self.repr_index(repr, span)?;
        let words = names.len().div_ceil(32);
        let scratch = self.scratch(context.depth);
        for word_index in 0..words {
            body.push(Op::Leaf(Instruction::I32Const(0)));
            for (label_index, label) in labels.iter().enumerate() {
                let Some(bit) = names
                    .iter()
                    .position(|name| abi::source_field_name(name) == *label)
                else {
                    return Err(wasm_error(span, "canonical flags element has no field"));
                };
                if bit / 32 != word_index {
                    continue;
                }
                let mut nested = path.to_vec();
                nested.push(Projection::Field {
                    ty: repr_index,
                    field: label_index as u32,
                });
                self.emit_project(body, context, &nested, span)?;
                let shift = bit % 32;
                if shift != 0 {
                    body.push(Op::Leaf(Instruction::I32Const(shift as i32)));
                    body.push(Op::Leaf(Instruction::I32Shl));
                }
                body.push(Op::Leaf(Instruction::I32Or));
            }
            body.push(Op::Leaf(Instruction::LocalSet(scratch)));
            self.element_address(body, context, offset + 4 * word_index as u32, span)?;
            body.push(Op::Leaf(Instruction::LocalGet(scratch)));
            store_flags_word(body, names.len(), words, true);
        }
        Ok(())
    }

    /// Unpacks a flags node into its boolean fields.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_flags_load(
        &self,
        body: &mut Body,
        context: &ListLoop,
        offset: u32,
        names: &[String],
        repr: crate::cc::ReprId,
        labels: &[String],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let repr_index = self.repr_index(repr, span)?;
        let words = names.len().div_ceil(32);
        for label in labels {
            let Some(bit) = names
                .iter()
                .position(|name| abi::source_field_name(name) == *label)
            else {
                return Err(wasm_error(span, "canonical flags element has no field"));
            };
            self.element_address(body, context, offset + 4 * (bit / 32) as u32, span)?;
            load_flags_word(body, names.len(), words);
            let shift = bit % 32;
            if shift != 0 {
                body.push(Op::Leaf(Instruction::I32Const(shift as i32)));
                body.push(Op::Leaf(Instruction::I32ShrU));
            }
            body.push(Op::Leaf(Instruction::I32Const(1)));
            body.push(Op::Leaf(Instruction::I32And));
        }
        body.push(Op::Leaf(Instruction::StructNew(repr_index)));
        Ok(())
    }
}

fn flags_width(count: usize) -> u32 {
    match count {
        1..=8 => 1,
        9..=16 => 2,
        _ => 4,
    }
}

fn store_flags_word(body: &mut Body, count: usize, words: usize, _value_on_stack: bool) {
    if words == 1 {
        match flags_width(count) {
            1 => body.push(Op::Leaf(Instruction::I32Store8(
                super::super::ops::memory_with_align(0, 0),
            ))),
            2 => body.push(Op::Leaf(Instruction::I32Store16(
                super::super::ops::memory_with_align(0, 1),
            ))),
            _ => body.push(Op::Leaf(Instruction::I32Store(super::super::ops::memory(
                0,
            )))),
        }
    } else {
        body.push(Op::Leaf(Instruction::I32Store(super::super::ops::memory(
            0,
        ))));
    }
}

fn load_flags_word(body: &mut Body, count: usize, words: usize) {
    if words == 1 {
        match flags_width(count) {
            1 => body.push(Op::Leaf(Instruction::I32Load8U(
                super::super::ops::memory_with_align(0, 0),
            ))),
            2 => body.push(Op::Leaf(Instruction::I32Load16U(
                super::super::ops::memory_with_align(0, 1),
            ))),
            _ => body.push(Op::Leaf(Instruction::I32Load(super::super::ops::memory(0)))),
        }
    } else {
        body.push(Op::Leaf(Instruction::I32Load(super::super::ops::memory(0))));
    }
}

fn label_index(labels: &[String], label: &str, span: TextRange) -> Result<u32, Vec<BackendError>> {
    labels
        .iter()
        .position(|candidate| candidate == label)
        .map(|index| index as u32)
        .ok_or_else(|| wasm_error(span, "canonical list element has no matching field"))
}
