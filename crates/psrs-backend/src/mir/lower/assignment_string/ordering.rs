use super::super::{BlockId, FunctionLowerer, layout_error};
use crate::BackendError;
use crate::mir::instruction::Instruction;
use crate::mir::{NumericOp, Terminator};
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    /// Lexicographic order of canonical UTF-8 bytes agrees with Unicode scalar
    /// order. Strings entering this operation retain their checked String type.
    pub(in crate::mir::lower) fn lower_string_lt(
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
        let left = self.persist_field(current, left, span)?;
        let right = self.persist_field(current, right, span)?;
        let left_for_length = self.restore_field(current, left, span)?;
        let right_for_length = self.restore_field(current, right, span)?;
        let left_length = self.fresh(ValueType::I32);
        let right_length = self.fresh(ValueType::I32);
        self.append_instruction(
            current,
            Instruction::ArrayLen {
                destination: left_length,
                value: left_for_length,
                span,
            },
            span,
        )?;
        self.append_instruction(
            current,
            Instruction::ArrayLen {
                destination: right_length,
                value: right_for_length,
                span,
            },
            span,
        )?;

        let index = self.fresh(ValueType::I32);
        let header = self.new_block(vec![index]);
        let body = self.new_block(Vec::new());
        let compare_length = self.new_block(Vec::new());
        let different = self.new_block(Vec::new());
        let less = self.new_block(Vec::new());
        let greater_or_equal = self.new_block(Vec::new());
        let advance = self.new_block(Vec::new());
        let join = self.new_block(vec![destination]);

        let zero = self.constant(current, 0, span)?;
        self.set_terminator(
            current,
            Terminator::Jump {
                target: header,
                arguments: vec![zero],
                span,
            },
            span,
        )?;

        let in_left = self.fresh(ValueType::Boolean);
        self.append_instruction(
            header,
            Instruction::Primitive {
                destination: in_left,
                op: NumericOp::I32LtS,
                left: index,
                right: left_length,
                span,
            },
            span,
        )?;
        let in_right = self.fresh(ValueType::Boolean);
        self.append_instruction(
            header,
            Instruction::Primitive {
                destination: in_right,
                op: NumericOp::I32LtS,
                left: index,
                right: right_length,
                span,
            },
            span,
        )?;
        let in_both = self.fresh(ValueType::Boolean);
        self.append_instruction(
            header,
            Instruction::Primitive {
                destination: in_both,
                op: NumericOp::BoolAnd,
                left: in_left,
                right: in_right,
                span,
            },
            span,
        )?;
        self.set_terminator(
            header,
            Terminator::Branch {
                condition: in_both,
                then_block: body,
                else_block: compare_length,
                span,
            },
            span,
        )?;
        let left_in_body = self.restore_field(body, left, span)?;
        let right_in_body = self.restore_field(body, right, span)?;
        let left_byte = self.fresh(ValueType::I32);
        self.append_instruction(
            body,
            Instruction::ArrayGetU {
                destination: left_byte,
                type_index: string_type,
                value: left_in_body,
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
                value: right_in_body,
                index,
                span,
            },
            span,
        )?;
        let same = self.fresh(ValueType::Boolean);
        self.append_instruction(
            body,
            Instruction::Primitive {
                destination: same,
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
                condition: same,
                then_block: advance,
                else_block: different,
                span,
            },
            span,
        )?;
        let byte_order = self.fresh(ValueType::Boolean);
        self.append_instruction(
            different,
            Instruction::Primitive {
                destination: byte_order,
                op: NumericOp::I32LtS,
                left: left_byte,
                right: right_byte,
                span,
            },
            span,
        )?;
        self.set_terminator(
            different,
            Terminator::Branch {
                condition: byte_order,
                then_block: less,
                else_block: greater_or_equal,
                span,
            },
            span,
        )?;
        let next = self.fresh(ValueType::I32);
        let one = self.constant(advance, 1, span)?;
        self.append_instruction(
            advance,
            Instruction::Primitive {
                destination: next,
                op: NumericOp::I32Add,
                left: index,
                right: one,
                span,
            },
            span,
        )?;
        self.set_terminator(
            advance,
            Terminator::Jump {
                target: header,
                arguments: vec![next],
                span,
            },
            span,
        )?;

        let left_shorter = self.fresh(ValueType::Boolean);
        self.append_instruction(
            compare_length,
            Instruction::Primitive {
                destination: left_shorter,
                op: NumericOp::I32LtS,
                left: left_length,
                right: right_length,
                span,
            },
            span,
        )?;
        self.finish_boolean(less, join, true, span)?;
        self.finish_boolean(greater_or_equal, join, false, span)?;
        let length_true = self.new_block(Vec::new());
        let length_false = self.new_block(Vec::new());
        self.set_terminator(
            compare_length,
            Terminator::Branch {
                condition: left_shorter,
                then_block: length_true,
                else_block: length_false,
                span,
            },
            span,
        )?;
        self.finish_boolean(length_true, join, true, span)?;
        self.finish_boolean(length_false, join, false, span)?;
        Ok(join)
    }

    fn finish_boolean(
        &mut self,
        block: BlockId,
        join: BlockId,
        value: bool,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
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
        )
    }
}
