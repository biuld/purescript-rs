//! Wasm lowering for MIR memory and width instructions.

use super::*;

impl Structurer<'_> {
    pub(super) fn emit_memory_instruction(
        &self,
        instruction: &MirInstruction,
        body: &mut Body,
    ) -> Result<(), Vec<BackendError>> {
        match instruction {
            MirInstruction::Load {
                destination,
                address,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                body.push(Op::Leaf(Instruction::I32Load(memory(*offset))));
                self.store(body, *destination, *span)?;
            }
            MirInstruction::Load8U {
                destination,
                address,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                body.push(Op::Leaf(Instruction::I32Load8U(memory_with_align(
                    *offset, 0,
                ))));
                self.store(body, *destination, *span)?;
            }
            MirInstruction::Load16U {
                destination,
                address,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                body.push(Op::Leaf(Instruction::I32Load16U(memory_with_align(
                    *offset, 1,
                ))));
                self.store(body, *destination, *span)?;
            }
            MirInstruction::LoadI64 {
                destination,
                address,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                body.push(Op::Leaf(Instruction::I64Load(memory_with_align(
                    *offset, 3,
                ))));
                self.store(body, *destination, *span)?;
            }
            MirInstruction::LoadF32 {
                destination,
                address,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                body.push(Op::Leaf(Instruction::F32Load(memory_with_align(
                    *offset, 2,
                ))));
                self.store(body, *destination, *span)?;
            }
            MirInstruction::LoadF64 {
                destination,
                address,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                body.push(Op::Leaf(Instruction::F64Load(memory_with_align(
                    *offset, 3,
                ))));
                self.store(body, *destination, *span)?;
            }
            MirInstruction::Store {
                address,
                value,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                self.load(body, *value, *span)?;
                body.push(Op::Leaf(Instruction::I32Store(memory(*offset))));
            }
            MirInstruction::Store8 {
                address,
                value,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                self.load(body, *value, *span)?;
                body.push(Op::Leaf(Instruction::I32Store8(memory_with_align(
                    *offset, 0,
                ))));
            }
            MirInstruction::Store16 {
                address,
                value,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                self.load(body, *value, *span)?;
                body.push(Op::Leaf(Instruction::I32Store16(memory_with_align(
                    *offset, 1,
                ))));
            }
            MirInstruction::StoreI64 {
                address,
                value,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                self.load(body, *value, *span)?;
                body.push(Op::Leaf(Instruction::I64Store(memory_with_align(
                    *offset, 3,
                ))));
            }
            MirInstruction::StoreF32 {
                address,
                value,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                self.load(body, *value, *span)?;
                body.push(Op::Leaf(Instruction::F32Store(memory_with_align(
                    *offset, 2,
                ))));
            }
            MirInstruction::StoreF64 {
                address,
                value,
                offset,
                span,
                ..
            } => {
                self.load(body, *address, *span)?;
                self.load(body, *value, *span)?;
                body.push(Op::Leaf(Instruction::F64Store(memory_with_align(
                    *offset, 3,
                ))));
            }
            MirInstruction::WrapI64 {
                destination,
                value,
                span,
            } => {
                self.load(body, *value, *span)?;
                body.push(Op::Leaf(Instruction::I32WrapI64));
                self.store(body, *destination, *span)?;
            }
            MirInstruction::WidenI64 {
                destination,
                value,
                signed,
                span,
            } => {
                self.load(body, *value, *span)?;
                body.push(Op::Leaf(if *signed {
                    Instruction::I64ExtendI32S
                } else {
                    Instruction::I64ExtendI32U
                }));
                self.store(body, *destination, *span)?;
            }
            _ => unreachable!("memory instruction lowering received another instruction"),
        }
        Ok(())
    }
}
