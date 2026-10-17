use super::*;
use crate::cc::RecordOperation;

impl FunctionLowerer<'_> {
    pub(in crate::mir::lower) fn lower_record_operation(
        &mut self,
        mut block: BlockId,
        destination: ValueId,
        record: ValueId,
        operation: &RecordOperation,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let storage = self
            .layout
            .representation_table()
            .record_storage()
            .ok_or_else(|| aggregate_error(span, "record operation has no storage schema"))?;
        let arrays = self.record_arrays(block, record, storage, span)?;
        let mut lengths = Vec::new();
        for array in arrays {
            let value = self.restore_field(block, array, span)?;
            let length = self.fresh(ValueType::I32);
            self.append_instruction(
                block,
                Instruction::ArrayLen {
                    destination: length,
                    value,
                    span,
                },
                span,
            )?;
            lengths.push(length);
        }
        let valid = self.fresh(ValueType::Boolean);
        self.append_instruction(
            block,
            Instruction::Primitive {
                destination: valid,
                op: NumericOp::I32Eq,
                left: lengths[0],
                right: lengths[1],
                span,
            },
            span,
        )?;
        block = self.require_record_condition(block, valid, span)?;
        let zero = self.record_integer(block, 0, span)?;
        let bounded = self.fresh(ValueType::Boolean);
        self.append_instruction(
            block,
            Instruction::Primitive {
                destination: bounded,
                op: NumericOp::I32GeS,
                left: lengths[0],
                right: zero,
                span,
            },
            span,
        )?;
        block = self.require_record_condition(block, bounded, span)?;
        match operation {
            RecordOperation::Length => {
                self.append_instruction(
                    block,
                    Instruction::Copy {
                        destination,
                        value: lengths[0],
                        span,
                    },
                    span,
                )?;
                Ok(block)
            }
            RecordOperation::Read { index } | RecordOperation::LabelAt { index } => {
                let valid = self.fresh(ValueType::Boolean);
                self.append_instruction(
                    block,
                    Instruction::Primitive {
                        destination: valid,
                        op: NumericOp::I32GeS,
                        left: *index,
                        right: zero,
                        span,
                    },
                    span,
                )?;
                block = self.require_record_condition(block, valid, span)?;
                let valid = self.fresh(ValueType::Boolean);
                self.append_instruction(
                    block,
                    Instruction::Primitive {
                        destination: valid,
                        op: NumericOp::I32LtS,
                        left: *index,
                        right: lengths[0],
                        span,
                    },
                    span,
                )?;
                block = self.require_record_condition(block, valid, span)?;
                let reading = matches!(operation, RecordOperation::Read { .. });
                let (array, repr) = if reading {
                    (arrays[1], storage.values())
                } else {
                    (arrays[0], storage.labels())
                };
                let array = self.restore_field(block, array, span)?;
                self.lower_array_get(block, destination, repr, array, *index, span)?;
                Ok(block)
            }
            RecordOperation::Find { label } => {
                let label = self.persist_field(block, *label, span)?;
                self.find_record_label(
                    block,
                    destination,
                    storage,
                    arrays[0],
                    label,
                    lengths[0],
                    span,
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn find_record_label(
        &mut self,
        block: BlockId,
        destination: ValueId,
        storage: RecordStorage,
        labels: StoredValue,
        label: StoredValue,
        length: ValueId,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let index = self.fresh(ValueType::I32);
        let header = self.new_block(vec![index]);
        let body = self.new_block(Vec::new());
        let found = self.new_block(Vec::new());
        let advance = self.new_block(Vec::new());
        let missing = self.new_block(Vec::new());
        let exit = self.new_block(vec![destination]);
        let zero = self.record_integer(block, 0, span)?;
        self.set_terminator(
            block,
            Terminator::Jump {
                target: header,
                arguments: vec![zero],
                span,
            },
            span,
        )?;
        let in_range = self.fresh(ValueType::Boolean);
        self.append_instruction(
            header,
            Instruction::Primitive {
                destination: in_range,
                op: NumericOp::I32LtS,
                left: index,
                right: length,
                span,
            },
            span,
        )?;
        self.set_terminator(
            header,
            Terminator::Branch {
                condition: in_range,
                then_block: body,
                else_block: missing,
                span,
            },
            span,
        )?;
        let array = self.restore_field(body, labels, span)?;
        let candidate = self.fresh(
            self.layout
                .value_type(&ValueShape::String)
                .map_err(|error| layout_error(span, error))?,
        );
        self.lower_array_get(body, candidate, storage.labels(), array, index, span)?;
        let label = self.restore_field(body, label, span)?;
        let equal = self.fresh(ValueType::Boolean);
        let compare = self.lower_string_eq(body, equal, candidate, label, span)?;
        self.set_terminator(
            compare,
            Terminator::Branch {
                condition: equal,
                then_block: found,
                else_block: advance,
                span,
            },
            span,
        )?;
        self.set_terminator(
            found,
            Terminator::Jump {
                target: exit,
                arguments: vec![index],
                span,
            },
            span,
        )?;
        let one = self.record_integer(advance, 1, span)?;
        let next = self.fresh(ValueType::I32);
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
        let absent = self.record_integer(missing, -1, span)?;
        self.set_terminator(
            missing,
            Terminator::Jump {
                target: exit,
                arguments: vec![absent],
                span,
            },
            span,
        )?;
        Ok(exit)
    }
}
