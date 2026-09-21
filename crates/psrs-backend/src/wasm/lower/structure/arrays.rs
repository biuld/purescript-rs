//! Wasm instruction emission for array and linear-memory copying.

use super::Structurer;
use super::helpers::ValueOps;
use crate::BackendError;
use crate::types::{DefinedTypeId, ValueId};
use crate::wasm::{Body, Op};
use wasm_encoder::Instruction;

impl Structurer<'_> {
    pub(super) fn emit_array_new(
        &self,
        body: &mut Body,
        destination: ValueId,
        type_index: DefinedTypeId,
        elements: &[ValueId],
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        for element in elements {
            self.load(body, *element, span)?;
        }
        body.push(Op::Leaf(Instruction::ArrayNewFixed {
            array_type_index: type_index.0,
            array_size: elements.len() as u32,
        }));
        self.store(body, destination, span)
    }

    pub(super) fn emit_array_get(
        &self,
        body: &mut Body,
        destination: ValueId,
        type_index: DefinedTypeId,
        value: ValueId,
        index: ValueId,
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, value, span)?;
        self.load(body, index, span)?;
        body.push(Op::Leaf(Instruction::ArrayGet(type_index.0)));
        self.store(body, destination, span)
    }

    pub(super) fn emit_array_set(
        &self,
        body: &mut Body,
        type_index: DefinedTypeId,
        value: ValueId,
        index: ValueId,
        new_value: ValueId,
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, value, span)?;
        self.load(body, index, span)?;
        self.load(body, new_value, span)?;
        body.push(Op::Leaf(Instruction::ArraySet(type_index.0)));
        Ok(())
    }

    pub(super) fn emit_array_len(
        &self,
        body: &mut Body,
        destination: ValueId,
        value: ValueId,
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, value, span)?;
        body.push(Op::Leaf(Instruction::ArrayLen));
        self.store(body, destination, span)
    }

    pub(super) fn emit_array_clone(
        &self,
        body: &mut Body,
        destination: ValueId,
        type_index: DefinedTypeId,
        value: ValueId,
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, value, span)?;
        body.push(Op::Leaf(Instruction::ArrayLen));
        body.push(Op::Leaf(Instruction::ArrayNewDefault(type_index.0)));
        self.store(body, destination, span)?;
        self.load(body, destination, span)?;
        body.push(Op::Leaf(Instruction::I32Const(0)));
        self.load(body, value, span)?;
        body.push(Op::Leaf(Instruction::I32Const(0)));
        self.load(body, value, span)?;
        body.push(Op::Leaf(Instruction::ArrayLen));
        body.push(Op::Leaf(Instruction::ArrayCopy {
            array_type_index_dst: type_index.0,
            array_type_index_src: type_index.0,
        }));
        Ok(())
    }

    pub(super) fn emit_linear_alloc_dynamic(
        &self,
        body: &mut Body,
        destination: ValueId,
        bytes: ValueId,
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let allocator = self.linear_allocator.ok_or_else(|| {
            super::super::wasm_error(span, "linear allocation has no allocator function")
        })?;
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32Const(4)));
        self.load(body, bytes, span)?;
        body.push(Op::Leaf(Instruction::Call(allocator.0)));
        self.store(body, destination, span)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_linear_memory_copy(
        &self,
        body: &mut Body,
        destination: ValueId,
        source: ValueId,
        bytes: ValueId,
        destination_offset: u32,
        source_offset: u32,
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, destination, span)?;
        if destination_offset != 0 {
            body.push(Op::Leaf(Instruction::I32Const(destination_offset as i32)));
            body.push(Op::Leaf(Instruction::I32Add));
        }
        self.load(body, source, span)?;
        if source_offset != 0 {
            body.push(Op::Leaf(Instruction::I32Const(source_offset as i32)));
            body.push(Op::Leaf(Instruction::I32Add));
        }
        self.load(body, bytes, span)?;
        body.push(Op::Leaf(Instruction::MemoryCopy {
            src_mem: 0,
            dst_mem: 0,
        }));
        Ok(())
    }
}
