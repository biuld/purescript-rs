use super::super::{BlockId, FunctionLowerer, layout_error};
use crate::BackendError;
use crate::mir::instruction::Instruction;
use crate::mir::{NumericOp, Terminator};
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    /// Compares canonical UTF-8 storage by byte length and byte content.
    /// String storage is a GC `array (mut i8)`; canonical UTF-8 gives each
    /// Unicode scalar sequence one byte sequence, so this is scalar equality.
    pub(in crate::mir::lower) fn lower_string_eq(
        &mut self,
        current: BlockId,
        destination: ValueId,
        left: ValueId,
        right: ValueId,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let string_type = self.layout.string_index().ok_or_else(|| {
            layout_error(span, crate::mir::layout::LayoutError::UnknownRepresentation)
        })?;
        let left_length = self.fresh(ValueType::I32);
        let right_length = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::ArrayLen {
                destination: left_length,
                value: left,
                span,
            },
            span,
        )?;
        self.append_instruction(
            current,
            Instruction::ArrayLen {
                destination: right_length,
                value: right,
                span,
            },
            span,
        )?;
        let same_length = self.fresh(ValueType::Boolean);
        self.append_instruction(
            current,
            Instruction::Primitive {
                destination: same_length,
                op: NumericOp::I32Eq,
                left: left_length,
                right: right_length,
                span,
            },
            span,
        )?;

        let start = self.new_block(Vec::new());
        let index = self.fresh(ValueType::I32);
        let header = self.new_block(vec![index]);
        let body = self.new_block(Vec::new());
        let advance = self.new_block(Vec::new());
        let equal = self.new_block(Vec::new());
        let different = self.new_block(Vec::new());
        let join = self.new_block(vec![destination]);
        self.set_terminator(
            current,
            Terminator::Branch {
                condition: same_length,
                then_block: start,
                else_block: different,
                span,
            },
            span,
        )?;
        let zero = self.constant(start, 0, span)?;
        self.set_terminator(
            start,
            Terminator::Jump {
                target: header,
                arguments: vec![zero],
                span,
            },
            span,
        )?;
        self.loop_header(header, body, equal, index, left_length, span)?;

        let left_byte = self.fresh(ValueType::I32);
        self.append_instruction(
            body,
            Instruction::ArrayGetU {
                destination: left_byte,
                type_index: string_type,
                value: left,
                index,
                span,
            },
            span,
        )?;
        let right_byte = self.fresh(ValueType::I32);
        self.append_instruction(
            body,
            Instruction::ArrayGetU {
                destination: right_byte,
                type_index: string_type,
                value: right,
                index,
                span,
            },
            span,
        )?;
        let same_byte = self.fresh(ValueType::Boolean);
        self.append_instruction(
            body,
            Instruction::Primitive {
                destination: same_byte,
                op: NumericOp::I32Eq,
                left: left_byte,
                right: right_byte,
                span,
            },
            span,
        )?;
        self.set_terminator(
            body,
            Terminator::Branch {
                condition: same_byte,
                then_block: advance,
                else_block: different,
                span,
            },
            span,
        )?;
        self.loop_step(advance, header, index, span)?;

        for (block, value) in [(equal, true), (different, false)] {
            let result = self.fresh(ValueType::Boolean);
            self.append_instruction(
                block,
                Instruction::Constant {
                    destination: result,
                    value: i32::from(value),
                    span,
                },
                span,
            )?;
            self.set_terminator(
                block,
                Terminator::Jump {
                    target: join,
                    arguments: vec![result],
                    span,
                },
                span,
            )?;
        }
        Ok(join)
    }
}
