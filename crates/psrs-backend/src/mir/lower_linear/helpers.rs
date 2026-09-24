use super::LinearFunctionLowerer;
use crate::BackendError;
use crate::mir::{BasicBlock, BlockId, Instruction, NumericOp, Terminator};
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;

fn unsupported(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P9 MIR lowering", span, message)]
}

pub(super) fn closure_layout(
    capture_count: usize,
    span: TextRange,
) -> Result<(Vec<u32>, u32), Vec<BackendError>> {
    let count = u32::try_from(capture_count)
        .map_err(|_| unsupported(span, "linear closure has too many captures"))?;
    let bytes = count
        .checked_mul(8)
        .and_then(|bytes| bytes.checked_add(8))
        .ok_or_else(|| unsupported(span, "linear closure is too large"))?;
    let offsets = (0..count).map(|index| 8 + index * 8).collect();
    Ok((offsets, bytes))
}

pub(super) fn closure_capture_offset(
    index: u32,
    span: TextRange,
) -> Result<u32, Vec<BackendError>> {
    index
        .checked_mul(8)
        .and_then(|offset| offset.checked_add(8))
        .ok_or_else(|| unsupported(span, "linear closure capture index is too large"))
}

pub(super) fn check_array_index(
    lowerer: &mut LinearFunctionLowerer<'_>,
    block: BlockId,
    array: ValueId,
    index: ValueId,
    object_bytes: u32,
    span: TextRange,
) -> Result<(), Vec<BackendError>> {
    let length = lowerer.fresh(ValueType::I32);
    lowerer.append(
        block,
        Instruction::LinearLoad {
            destination: length,
            address: array,
            memory: crate::types::MemoryId(0),
            offset: 0,
            object_bytes,
            alignment: 4,
            ty: ValueType::I32,
            span,
        },
        span,
    )?;
    let zero = lowerer.fresh(ValueType::I32);
    lowerer.append(
        block,
        Instruction::Constant {
            destination: zero,
            value: 0,
            span,
        },
        span,
    )?;
    let negative = lowerer.fresh(ValueType::Boolean);
    lowerer.append(
        block,
        Instruction::Primitive {
            destination: negative,
            op: NumericOp::I32LtS,
            left: index,
            right: zero,
            span,
        },
        span,
    )?;
    lowerer.append(
        block,
        Instruction::TrapIf {
            condition: negative,
            span,
        },
        span,
    )?;
    let past_end = lowerer.fresh(ValueType::Boolean);
    lowerer.append(
        block,
        Instruction::Primitive {
            destination: past_end,
            op: NumericOp::I32GeS,
            left: index,
            right: length,
            span,
        },
        span,
    )?;
    lowerer.append(
        block,
        Instruction::TrapIf {
            condition: past_end,
            span,
        },
        span,
    )
}

impl LinearFunctionLowerer<'_> {
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
    pub(super) fn index_address(
        &mut self,
        block: BlockId,
        base: ValueId,
        index: ValueId,
        stride: u32,
        element_offset: u32,
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
                    op: NumericOp::I32Mul,
                    left: index,
                    right: stride_value,
                    span,
                },
                span,
            )?;
            scaled
        };
        let payload_offset = if element_offset == 0 {
            scaled
        } else {
            let element_offset_value = self.fresh(ValueType::I32);
            self.append(
                block,
                Instruction::Constant {
                    destination: element_offset_value,
                    value: element_offset as i32,
                    span,
                },
                span,
            )?;
            let payload_offset = self.fresh(ValueType::I32);
            self.append(
                block,
                Instruction::Primitive {
                    destination: payload_offset,
                    op: NumericOp::I32Add,
                    left: scaled,
                    right: element_offset_value,
                    span,
                },
                span,
            )?;
            payload_offset
        };
        let address = self.fresh(ValueType::I32);
        self.append(
            block,
            Instruction::Primitive {
                destination: address,
                op: NumericOp::I32Add,
                left: base,
                right: payload_offset,
                span,
            },
            span,
        )?;
        Ok(address)
    }

    pub(in crate::mir) fn append(
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
