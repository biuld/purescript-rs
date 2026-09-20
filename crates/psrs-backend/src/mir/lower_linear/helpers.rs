use super::LinearFunctionLowerer;
use crate::BackendError;
use crate::mir::{BasicBlock, BlockId, Instruction, Terminator};
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;

fn unsupported(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P9 MIR lowering", span, message)]
}

impl LinearFunctionLowerer<'_> {
    pub(super) fn index_address(
        &mut self,
        block: BlockId,
        base: ValueId,
        index: ValueId,
        stride: u32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let scaled = if stride == 1 {
            index
        } else {
            let stride_value = self.fresh(ValueType::I32);
            self.append(
                block,
                Instruction::Constant {
                    destination: stride_value,
                    value: stride as i32,
                    span,
                },
                span,
            )?;
            let scaled = self.fresh(ValueType::I32);
            self.append(
                block,
                Instruction::Primitive {
                    destination: scaled,
                    op: psrs_core::Primitive::Mul,
                    left: index,
                    right: stride_value,
                    span,
                },
                span,
            )?;
            scaled
        };
        let address = self.fresh(ValueType::I32);
        self.append(
            block,
            Instruction::Primitive {
                destination: address,
                op: psrs_core::Primitive::Add,
                left: base,
                right: scaled,
                span,
            },
            span,
        )?;
        Ok(address)
    }

    pub(super) fn new_block(&mut self, parameters: Vec<ValueId>) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.blocks.push(BasicBlock {
            id,
            parameters,
            instructions: Vec::new(),
            terminator: None,
        });
        id
    }

    pub(super) fn append(
        &mut self,
        block: BlockId,
        instruction: Instruction,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let target = self
            .blocks
            .iter_mut()
            .find(|candidate| candidate.id == block)
            .ok_or_else(|| unsupported(span, "linear-memory block ID was not allocated"))?;
        if target.terminator.is_some() {
            return Err(unsupported(span, "cannot append after a MIR terminator"));
        }
        target.instructions.push(instruction);
        Ok(())
    }

    pub(super) fn set_terminator(
        &mut self,
        block: BlockId,
        terminator: Terminator,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let target = self
            .blocks
            .iter_mut()
            .find(|candidate| candidate.id == block)
            .ok_or_else(|| unsupported(span, "linear-memory block ID was not allocated"))?;
        if target.terminator.replace(terminator).is_some() {
            return Err(unsupported(span, "basic block already has a terminator"));
        }
        Ok(())
    }
}
