//! Wasm instruction emission for array and linear-memory copying.

use super::Structurer;
use super::helpers::ValueOps;
use crate::BackendError;
use crate::mir::{Function as MirFunction, Instruction as MirInstruction};
use crate::types::{DefinedTypeId, ValueId};
use crate::wasm::{Body, Op};
use wasm_encoder::Instruction;

pub(super) fn linear_copy_local_indices(
    function: &MirFunction,
) -> Result<Option<[u32; 3]>, Vec<BackendError>> {
    let needs_temporaries = function.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, MirInstruction::LinearMemoryCopy { .. }))
    });
    if !needs_temporaries {
        return Ok(None);
    }
    let first = u32::try_from(function.values.len())
        .map_err(|_| super::wasm_error(function.span, "MIR function has too many locals"))?;
    let second = first
        .checked_add(1)
        .ok_or_else(|| super::wasm_error(function.span, "MIR function has too many locals"))?;
    let third = first
        .checked_add(2)
        .ok_or_else(|| super::wasm_error(function.span, "MIR function has too many locals"))?;
    Ok(Some([first, second, third]))
}

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
        alignment: u32,
        span: psrs_span::TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let allocator = self.linear_allocator.ok_or_else(|| {
            super::super::wasm_error(span, "linear allocation has no allocator function")
        })?;
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::I32Const(alignment as i32)));
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
        let [destination_local, source_local, remaining_local] = self
            .linear_copy_locals
            .ok_or_else(|| super::super::wasm_error(span, "linear copy has no temporary locals"))?;
        self.load(body, destination, span)?;
        if destination_offset != 0 {
            body.push(Op::Leaf(Instruction::I32Const(destination_offset as i32)));
            body.push(Op::Leaf(Instruction::I32Add));
        }
        body.push(Op::Leaf(Instruction::LocalSet(destination_local)));
        self.load(body, source, span)?;
        if source_offset != 0 {
            body.push(Op::Leaf(Instruction::I32Const(source_offset as i32)));
            body.push(Op::Leaf(Instruction::I32Add));
        }
        body.push(Op::Leaf(Instruction::LocalSet(source_local)));
        self.load(body, bytes, span)?;
        body.push(Op::Leaf(Instruction::LocalSet(remaining_local)));

        // Expand memory.copy as a core-MVP memmove loop. This keeps the MIR
        // operation's overlap semantics without requiring bulk-memory.
        let byte = wasm_encoder::MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        };
        body.push(Op::Leaf(Instruction::Block(wasm_encoder::BlockType::Empty)));
        body.push(Op::Leaf(Instruction::LocalGet(remaining_local)));
        body.push(Op::Leaf(Instruction::I32Eqz));
        body.push(Op::Leaf(Instruction::BrIf(0)));

        body.push(Op::Leaf(Instruction::LocalGet(destination_local)));
        body.push(Op::Leaf(Instruction::LocalGet(source_local)));
        body.push(Op::Leaf(Instruction::I32GtU));
        body.push(Op::Leaf(Instruction::LocalGet(destination_local)));
        body.push(Op::Leaf(Instruction::LocalGet(source_local)));
        body.push(Op::Leaf(Instruction::LocalGet(remaining_local)));
        body.push(Op::Leaf(Instruction::I32Add));
        body.push(Op::Leaf(Instruction::I32LtU));
        body.push(Op::Leaf(Instruction::I32And));
        body.push(Op::Leaf(Instruction::If(wasm_encoder::BlockType::Empty)));
        for pointer_local in [destination_local, source_local] {
            body.push(Op::Leaf(Instruction::LocalGet(pointer_local)));
            body.push(Op::Leaf(Instruction::LocalGet(remaining_local)));
            body.push(Op::Leaf(Instruction::I32Add));
            body.push(Op::Leaf(Instruction::I32Const(1)));
            body.push(Op::Leaf(Instruction::I32Sub));
            body.push(Op::Leaf(Instruction::LocalSet(pointer_local)));
        }
        body.push(Op::Leaf(Instruction::Block(wasm_encoder::BlockType::Empty)));
        body.push(Op::Leaf(Instruction::Loop(wasm_encoder::BlockType::Empty)));
        emit_copy_loop_body(
            body,
            destination_local,
            source_local,
            remaining_local,
            byte,
            false,
        );
        body.push(Op::Leaf(Instruction::End));
        body.push(Op::Leaf(Instruction::End));
        body.push(Op::Leaf(Instruction::Else));
        body.push(Op::Leaf(Instruction::Block(wasm_encoder::BlockType::Empty)));
        body.push(Op::Leaf(Instruction::Loop(wasm_encoder::BlockType::Empty)));
        emit_copy_loop_body(
            body,
            destination_local,
            source_local,
            remaining_local,
            byte,
            true,
        );
        body.push(Op::Leaf(Instruction::End));
        body.push(Op::Leaf(Instruction::End));
        body.push(Op::Leaf(Instruction::End));
        body.push(Op::Leaf(Instruction::End));
        Ok(())
    }
}

fn emit_copy_loop_body(
    body: &mut Body,
    destination: u32,
    source: u32,
    remaining: u32,
    byte: wasm_encoder::MemArg,
    forward: bool,
) {
    body.push(Op::Leaf(Instruction::LocalGet(remaining)));
    body.push(Op::Leaf(Instruction::I32Eqz));
    body.push(Op::Leaf(Instruction::BrIf(1)));
    body.push(Op::Leaf(Instruction::LocalGet(destination)));
    body.push(Op::Leaf(Instruction::LocalGet(source)));
    body.push(Op::Leaf(Instruction::I32Load8U(byte)));
    body.push(Op::Leaf(Instruction::I32Store8(byte)));
    let step = 1;
    for pointer in [destination, source] {
        body.push(Op::Leaf(Instruction::LocalGet(pointer)));
        body.push(Op::Leaf(Instruction::I32Const(step)));
        body.push(Op::Leaf(if forward {
            Instruction::I32Add
        } else {
            Instruction::I32Sub
        }));
        body.push(Op::Leaf(Instruction::LocalSet(pointer)));
    }
    body.push(Op::Leaf(Instruction::LocalGet(remaining)));
    body.push(Op::Leaf(Instruction::I32Const(1)));
    body.push(Op::Leaf(Instruction::I32Sub));
    body.push(Op::Leaf(Instruction::LocalSet(remaining)));
    body.push(Op::Leaf(Instruction::Br(0)));
}
