use super::super::helpers::ValueOps;
use super::super::wasm_error;
use super::Structurer;
use crate::BackendError;
use crate::abi::layout::SlotKind;
use crate::abi::{self};
use crate::mir::{Instruction as MirInstruction, ListDirection};
use crate::types::ValueId;
use crate::wasm::{Body, Op};
use psrs_span::TextRange;
use wasm_encoder::Instruction;

impl Structurer<'_> {
    pub(in crate::wasm::lower::structure) fn emit_list_copy_record(
        &self,
        instruction: &MirInstruction,
    ) -> Result<Body, Vec<BackendError>> {
        let MirInstruction::ListCopyRecord {
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
            unreachable!("record list copy received another instruction")
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
            ListDirection::Store => self.emit_record_store(
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
            ListDirection::Load => self.emit_record_load(
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
            ListDirection::FreeStrings => self.emit_record_free_strings(
                &mut loop_body,
                *pointer,
                *size,
                fields,
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

    #[allow(clippy::too_many_arguments)]
    fn emit_record_store(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        struct_type: u32,
        pointer: ValueId,
        size: u32,
        fields: &[crate::mir::ListFieldCopy],
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        for field in fields {
            match field {
                crate::mir::ListFieldCopy::Scalar {
                    offset,
                    index,
                    kind,
                } => {
                    self.element_address(body, pointer, index_local, size as i32, span)?;
                    self.array_get(body, array, array_type, index_local, span)?;
                    body.push(Op::Leaf(Instruction::StructGet {
                        struct_type_index: struct_type,
                        field_index: *index,
                    }));
                    store_slot_kind(body, *kind, *offset);
                }
                crate::mir::ListFieldCopy::String { offset, index } => {
                    let encode = self.function_index(abi::STRING_TO_BYTES_SYMBOL, span)?;
                    self.array_get(body, array, array_type, index_local, span)?;
                    body.push(Op::Leaf(Instruction::StructGet {
                        struct_type_index: struct_type,
                        field_index: *index,
                    }));
                    body.push(Op::Leaf(Instruction::RefAsNonNull));
                    body.push(Op::Leaf(Instruction::Call(encode)));
                    body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
                    self.element_address(body, pointer, index_local, size as i32, span)?;
                    body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                    body.push(Op::Leaf(Instruction::I32Const(4)));
                    body.push(Op::Leaf(Instruction::I32Add));
                    body.push(Op::Leaf(Instruction::I32Store(
                        super::super::ops::memory_with_align(*offset, 2),
                    )));
                    self.element_address(body, pointer, index_local, size as i32, span)?;
                    body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                    body.push(Op::Leaf(Instruction::I32Load(
                        super::super::ops::memory_with_align(0, 2),
                    )));
                    body.push(Op::Leaf(Instruction::I32Store(
                        super::super::ops::memory_with_align(*offset + 4, 2),
                    )));
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_record_load(
        &self,
        body: &mut Body,
        array: ValueId,
        array_type: u32,
        struct_type: u32,
        pointer: ValueId,
        size: u32,
        fields: &[crate::mir::ListFieldCopy],
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.element_address(body, pointer, index_local, size as i32, span)?;
        body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        self.load(body, array, span)?;
        body.push(Op::Leaf(Instruction::LocalGet(index_local)));
        for field in fields {
            match field {
                crate::mir::ListFieldCopy::Scalar {
                    offset,
                    index: _,
                    kind,
                } => {
                    body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                    load_slot_kind(body, *kind, *offset);
                }
                crate::mir::ListFieldCopy::String { offset, index: _ } => {
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
                    // Free the host-allocated element buffer now that it is
                    // decoded into a GC string.
                    body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                    body.push(Op::Leaf(Instruction::I32Load(
                        super::super::ops::memory_with_align(*offset, 2),
                    )));
                    body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                    body.push(Op::Leaf(Instruction::I32Load(
                        super::super::ops::memory_with_align(*offset + 4, 2),
                    )));
                    let realloc = self.function_index(abi::REALLOC_SYMBOL, span)?;
                    body.push(Op::Leaf(Instruction::I32Const(1)));
                    body.push(Op::Leaf(Instruction::I32Const(0)));
                    body.push(Op::Leaf(Instruction::Call(realloc)));
                    body.push(Op::Leaf(Instruction::Drop));
                }
            }
        }
        body.push(Op::Leaf(Instruction::StructNew(struct_type)));
        body.push(Op::Leaf(Instruction::ArraySet(array_type)));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_record_free_strings(
        &self,
        body: &mut Body,
        pointer: ValueId,
        size: u32,
        fields: &[crate::mir::ListFieldCopy],
        index_local: u32,
        scratch_local: u32,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        self.element_address(body, pointer, index_local, size as i32, span)?;
        body.push(Op::Leaf(Instruction::LocalSet(scratch_local)));
        let realloc = self.function_index(abi::REALLOC_SYMBOL, span)?;
        for field in fields {
            if let crate::mir::ListFieldCopy::String { offset, .. } = field {
                body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                body.push(Op::Leaf(Instruction::I32Load(
                    super::super::ops::memory_with_align(*offset, 2),
                )));
                body.push(Op::Leaf(Instruction::LocalGet(scratch_local)));
                body.push(Op::Leaf(Instruction::I32Load(
                    super::super::ops::memory_with_align(*offset + 4, 2),
                )));
                body.push(Op::Leaf(Instruction::I32Const(1)));
                body.push(Op::Leaf(Instruction::I32Const(0)));
                body.push(Op::Leaf(Instruction::Call(realloc)));
                body.push(Op::Leaf(Instruction::Drop));
            }
        }
        Ok(())
    }
}

fn store_slot_kind(body: &mut Body, kind: SlotKind, offset: u32) {
    let mem = super::super::ops::memory_with_align;
    match kind {
        SlotKind::Byte => body.push(Op::Leaf(Instruction::I32Store8(mem(offset, 0)))),
        SlotKind::Half => body.push(Op::Leaf(Instruction::I32Store16(mem(offset, 1)))),
        SlotKind::Word => body.push(Op::Leaf(Instruction::I32Store(mem(offset, 2)))),
        SlotKind::F64 => body.push(Op::Leaf(Instruction::F64Store(mem(offset, 3)))),
        SlotKind::I64 | SlotKind::F32 => {
            unreachable!("record list elements carry only i32 and f64 fields")
        }
    }
}

fn load_slot_kind(body: &mut Body, kind: SlotKind, offset: u32) {
    let mem = super::super::ops::memory_with_align;
    match kind {
        SlotKind::Byte => body.push(Op::Leaf(Instruction::I32Load8U(mem(offset, 0)))),
        SlotKind::Half => body.push(Op::Leaf(Instruction::I32Load16U(mem(offset, 1)))),
        SlotKind::Word => body.push(Op::Leaf(Instruction::I32Load(mem(offset, 2)))),
        SlotKind::F64 => body.push(Op::Leaf(Instruction::F64Load(mem(offset, 3)))),
        SlotKind::I64 | SlotKind::F32 => {
            unreachable!("record list elements carry only i32 and f64 fields")
        }
    }
}
