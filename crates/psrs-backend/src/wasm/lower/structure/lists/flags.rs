use super::super::helpers::ValueOps;
use super::super::wasm_error;
use super::Structurer;
use crate::BackendError;
use crate::mir::{Instruction as MirInstruction, ListDirection};
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

impl Structurer<'_> {
    pub(in crate::wasm::lower::structure) fn emit_list_copy_flags(
        &self,
        instruction: &MirInstruction,
    ) -> Result<Body, Vec<BackendError>> {
        let MirInstruction::ListCopyFlags {
            direction,
            array,
            array_type,
            struct_type,
            pointer,
            length,
            size,
            fields,
            span,
        } = instruction
        else {
            unreachable!("flags list copy received another instruction")
        };
        let Some((index_local, scratch_local)) = self.list_locals else {
            return Err(wasm_error(*span, "canonical list copy has no loop locals"));
        };
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
            ListDirection::Store => self.emit_flags_store(
                &mut loop_body,
                *array,
                array_type.0,
                struct_type.0,
                *pointer,
                *size,
                fields,
                index_local,
                scratch_local,
                *span,
            )?,
            ListDirection::Load => self.emit_flags_load(
                &mut loop_body,
                *array,
                array_type.0,
                struct_type.0,
                *pointer,
                *size,
                fields,
                index_local,
                scratch_local,
                *span,
            )?,
            ListDirection::FreeStrings => {}
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

    #[allow(clippy::too_many_arguments)]
    fn emit_flags_store(
        &self,
        body: &mut Body,
        array: crate::types::ValueId,
        array_type: u32,
        struct_type: u32,
        pointer: crate::types::ValueId,
        size: u32,
        fields: &[crate::mir::ListFlagsField],
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        body.push(Op::Leaf(Instruction::I32Const(0)));
        body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        for field in fields {
            self.array_get(body, array, array_type, index_local, span)?;
            body.push(Op::Leaf(Instruction::StructGet {
                struct_type_index: struct_type,
                field_index: field.index,
            }));
            if field.bit != 0 {
                body.push(Op::Leaf(Instruction::I32Const(field.bit as i32)));
                body.push(Op::Leaf(Instruction::I32Shl));
            }
            body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
            body.push(Op::Leaf(Instruction::I32Or));
            body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        }
        self.element_address(body, pointer, index_local, size as i32, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
        body.push(Op::Leaf(Instruction::I32Store(
            super::super::ops::memory_with_align(0, 2),
        )));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_flags_load(
        &self,
        body: &mut Body,
        array: crate::types::ValueId,
        array_type: u32,
        struct_type: u32,
        pointer: crate::types::ValueId,
        size: u32,
        fields: &[crate::mir::ListFlagsField],
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.element_address(body, pointer, index_local, size as i32, span)?;
        body.push(Op::Leaf(Instruction::I32Load(
            super::super::ops::memory_with_align(0, 2),
        )));
        body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        self.load(body, array, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        for field in fields {
            body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
            if field.bit != 0 {
                body.push(Op::Leaf(Instruction::I32Const(field.bit as i32)));
                body.push(Op::Leaf(Instruction::I32ShrU));
            }
            body.push(Op::Leaf(Instruction::I32Const(1)));
            body.push(Op::Leaf(Instruction::I32And));
        }
        body.push(Op::Leaf(Instruction::StructNew(struct_type)));
        body.push(Op::Leaf(Instruction::ArraySet(array_type)));
        Ok(())
    }
}
