//! Wasm instruction emission for GC arrays.

use super::Structurer;
use super::helpers::ValueOps;
use crate::BackendError;
use crate::mir::Instruction as MirInstruction;
use crate::types::{DefinedTypeId, ValueId};
use crate::wasm::{Body, Op};
use wasm_encoder::Instruction;

impl Structurer<'_> {
    pub(super) fn trap_if(
        &self,
        body: &mut Body,
        instruction: &MirInstruction,
    ) -> Result<(), Vec<BackendError>> {
        let MirInstruction::TrapIf { condition, span } = instruction else {
            unreachable!("trap helper received another instruction")
        };
        self.emit_trap_if(body, *condition, *span)
    }

    pub(super) fn emit_trap_if(
        &self,
        body: &mut Body,
        condition: ValueId,
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, condition, span)?;
        body.push(Op::Leaf(Instruction::If(wasm_encoder::BlockType::Empty)));
        body.push(Op::Leaf(Instruction::Unreachable));
        body.push(Op::Leaf(Instruction::End));
        Ok(())
    }

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

    pub(super) fn emit_array_new_default(
        &self,
        body: &mut Body,
        destination: ValueId,
        type_index: DefinedTypeId,
        length: ValueId,
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.load(body, length, span)?;
        body.push(Op::Leaf(Instruction::ArrayNewDefault(type_index.0)));
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

    pub(super) fn emit_array_get_u(
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
        body.push(Op::Leaf(Instruction::ArrayGetU(type_index.0)));
        self.store(body, destination, span)
    }

    /// Emits the allocation half of `stringToBytes`: a fresh `Array Int` as long
    /// as the source string. The element copy itself lives in the blocks the
    /// instruction names, which the region emitter structures with the rest of
    /// the function.
    pub(super) fn emit_string_bytes(
        &self,
        body: &mut Vec<Op>,
        instruction: &crate::mir::Instruction,
    ) -> Result<(), Vec<BackendError>> {
        let crate::mir::Instruction::StringToBytes {
            destination,
            type_index,
            value,
            span,
            ..
        } = instruction
        else {
            unreachable!("string/byte emission received another instruction")
        };
        self.load(body, *value, *span)?;
        body.push(Op::Leaf(Instruction::ArrayLen));
        body.push(Op::Leaf(Instruction::ArrayNewDefault(type_index.0)));
        self.store(body, *destination, *span)
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
}
