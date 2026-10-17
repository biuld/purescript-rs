use super::super::*;
use crate::cc::{RefShape, Reference, ValueShape};
use crate::mir::instruction::Instruction;
use crate::mir::{NumericOp, Terminator};
use crate::types::{ValueId, ValueType};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    /// Snapshot both checked source arrays, then verify equal length, non-null
    /// elements, and strictly increasing Unicode-scalar labels before making a
    /// fresh record header.
    pub(in crate::mir::lower) fn lower_record_build_operation(
        &mut self,
        block: BlockId,
        destination: ValueId,
        labels: ValueId,
        payloads: ValueId,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let storage = self
            .layout
            .representation_table()
            .record_storage()
            .ok_or_else(|| {
                aggregate_error(span, "record build has no registered storage schema")
            })?;
        let labels = self.clone_record_array(block, labels, storage.values(), span)?;
        let payloads = self.clone_record_array(block, payloads, storage.values(), span)?;
        let labels = self.persist_field(block, labels, span)?;
        let payloads = self.persist_field(block, payloads, span)?;
        let label_array_length = self.restore_field(block, labels, span)?;
        let payload_array_length = self.restore_field(block, payloads, span)?;
        let label_length = self.record_array_length(block, label_array_length, span)?;
        let payload_length = self.record_array_length(block, payload_array_length, span)?;
        let equal_lengths = self.fresh(ValueType::Boolean);
        self.append_instruction(
            block,
            Instruction::Primitive {
                destination: equal_lengths,
                op: NumericOp::I32Eq,
                left: label_length,
                right: payload_length,
                span,
            },
            span,
        )?;
        let mut block = self.require_record_condition(block, equal_lengths, span)?;
        let zero = self.record_integer(block, 0, span)?;
        let nonnegative = self.fresh(ValueType::Boolean);
        self.append_instruction(
            block,
            Instruction::Primitive {
                destination: nonnegative,
                op: NumericOp::I32GeS,
                left: label_length,
                right: zero,
                span,
            },
            span,
        )?;
        block = self.require_record_condition(block, nonnegative, span)?;

        let label_array = self.fresh(
            self.layout
                .value_type(&ValueShape::Reference(Reference {
                    nullable: true,
                    heap: RefShape::Repr(storage.labels()),
                }))
                .map_err(|error| layout_error(span, error))?,
        );
        self.append_instruction(
            block,
            Instruction::ArrayNewSized {
                destination: label_array,
                type_index: self
                    .layout
                    .repr_index(storage.labels())
                    .map_err(|error| layout_error(span, error))?,
                length: label_length,
                span,
            },
            span,
        )?;
        let label_array = self.persist_field(block, label_array, span)?;

        let body = self.new_block(Vec::new());
        let done = self.new_block(Vec::new());
        let index = self.fresh(ValueType::I32);
        let loop_header = self.new_block(vec![index]);
        let zero = self.record_integer(block, 0, span)?;
        self.set_terminator(
            block,
            Terminator::Jump {
                target: loop_header,
                arguments: vec![zero],
                span,
            },
            span,
        )?;
        let in_range = self.fresh(ValueType::Boolean);
        self.append_instruction(
            loop_header,
            Instruction::Primitive {
                destination: in_range,
                op: NumericOp::I32LtS,
                left: index,
                right: label_length,
                span,
            },
            span,
        )?;
        self.set_terminator(
            loop_header,
            Terminator::Branch {
                condition: in_range,
                then_block: body,
                else_block: done,
                span,
            },
            span,
        )?;

        let label_type = self
            .layout
            .value_type(&ValueShape::String)
            .map_err(|error| layout_error(span, error))?;
        let labels_at_body = self.restore_field(body, labels, span)?;
        let raw_current = self.fresh(
            self.layout
                .value_type(&erased_shape())
                .map_err(|error| layout_error(span, error))?,
        );
        self.lower_array_get(
            body,
            raw_current,
            storage.values(),
            labels_at_body,
            index,
            span,
        )?;
        let current = self.recover_record_label(body, raw_current, label_type, span)?;
        let current = self.persist_field(body, current, span)?;
        let has_previous = self.fresh(ValueType::Boolean);
        let zero = self.record_integer(body, 0, span)?;
        self.append_instruction(
            body,
            Instruction::Primitive {
                destination: has_previous,
                op: NumericOp::I32GtS,
                left: index,
                right: zero,
                span,
            },
            span,
        )?;
        let compare = self.new_block(Vec::new());
        let skip = self.new_block(Vec::new());
        let checked = self.new_block(Vec::new());
        self.set_terminator(
            body,
            Terminator::Branch {
                condition: has_previous,
                then_block: compare,
                else_block: skip,
                span,
            },
            span,
        )?;

        let previous_index = self.fresh(ValueType::I32);
        let one = self.record_integer(compare, 1, span)?;
        self.append_instruction(
            compare,
            Instruction::Primitive {
                destination: previous_index,
                op: NumericOp::I32Sub,
                left: index,
                right: one,
                span,
            },
            span,
        )?;
        let labels_at_compare = self.restore_field(compare, labels, span)?;
        let raw_previous = self.fresh(
            self.layout
                .value_type(&erased_shape())
                .map_err(|error| layout_error(span, error))?,
        );
        self.lower_array_get(
            compare,
            raw_previous,
            storage.values(),
            labels_at_compare,
            previous_index,
            span,
        )?;
        let previous = self.recover_record_label(compare, raw_previous, label_type, span)?;
        let previous = self.persist_field(compare, previous, span)?;
        let current_at_compare = self.restore_field(compare, current, span)?;
        let ordered = self.fresh(ValueType::Boolean);
        let previous = self.restore_field(compare, previous, span)?;
        let compare = self.lower_string_lt(compare, ordered, previous, current_at_compare, span)?;
        let compare = self.require_record_condition(compare, ordered, span)?;
        self.set_terminator(
            compare,
            Terminator::Jump {
                target: checked,
                arguments: Vec::new(),
                span,
            },
            span,
        )?;
        self.set_terminator(
            skip,
            Terminator::Jump {
                target: checked,
                arguments: Vec::new(),
                span,
            },
            span,
        )?;

        let current_at_checked = self.restore_field(checked, current, span)?;
        let label_array_at_checked = self.restore_field(checked, label_array, span)?;
        self.append_instruction(
            checked,
            Instruction::ArraySet {
                type_index: self
                    .layout
                    .repr_index(storage.labels())
                    .map_err(|error| layout_error(span, error))?,
                value: label_array_at_checked,
                index,
                new_value: current_at_checked,
                span,
            },
            span,
        )?;

        let nullable_payload = ValueShape::Reference(Reference {
            nullable: true,
            heap: RefShape::Erased,
        });
        let payload_type = self
            .layout
            .value_type(&nullable_payload)
            .map_err(|error| layout_error(span, error))?;
        let payload = self.fresh(payload_type);
        let payloads_at_checked = self.restore_field(checked, payloads, span)?;
        self.lower_array_get(
            checked,
            payload,
            storage.values(),
            payloads_at_checked,
            index,
            span,
        )?;
        let is_null = self.fresh(ValueType::Boolean);
        self.append_instruction(
            checked,
            Instruction::RefIsNull {
                destination: is_null,
                value: payload,
                span,
            },
            span,
        )?;
        let false_value = self.fresh(ValueType::Boolean);
        self.append_instruction(
            checked,
            Instruction::Constant {
                destination: false_value,
                value: 0,
                span,
            },
            span,
        )?;
        let nonnull = self.fresh(ValueType::Boolean);
        self.append_instruction(
            checked,
            Instruction::Primitive {
                destination: nonnull,
                op: NumericOp::BoolEq,
                left: is_null,
                right: false_value,
                span,
            },
            span,
        )?;
        let checked = self.require_record_condition(checked, nonnull, span)?;
        let next = self.fresh(ValueType::I32);
        let one = self.record_integer(checked, 1, span)?;
        self.append_instruction(
            checked,
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
            checked,
            Terminator::Jump {
                target: loop_header,
                arguments: vec![next],
                span,
            },
            span,
        )?;

        let label_array_at_done = self.restore_field(done, label_array, span)?;
        let labels = self.fresh(
            self.layout
                .value_type(&ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(storage.labels()),
                }))
                .map_err(|error| layout_error(span, error))?,
        );
        self.append_instruction(
            done,
            Instruction::RefCast {
                destination: labels,
                value: label_array_at_done,
                reference: crate::types::RefType {
                    nullable: false,
                    heap: crate::types::HeapType::Index(
                        self.layout
                            .repr_index(storage.labels())
                            .map_err(|error| layout_error(span, error))?,
                    ),
                },
                span,
            },
            span,
        )?;
        let payloads_at_done = self.restore_field(done, payloads, span)?;
        self.append_instruction(
            done,
            Instruction::StructNew {
                destination,
                type_index: self
                    .layout
                    .repr_index(storage.representation())
                    .map_err(|error| layout_error(span, error))?,
                arguments: vec![labels, payloads_at_done],
                span,
            },
            span,
        )?;
        Ok(done)
    }

    fn recover_record_label(
        &mut self,
        block: BlockId,
        value: ValueId,
        destination_type: ValueType,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let destination = self.fresh(destination_type);
        let string_type = self.layout.string_index().ok_or_else(|| {
            layout_error(span, crate::mir::layout::LayoutError::UnknownRepresentation)
        })?;
        self.append_instruction(
            block,
            Instruction::RefCast {
                destination,
                value,
                reference: crate::types::RefType {
                    nullable: false,
                    heap: crate::types::HeapType::Index(string_type),
                },
                span,
            },
            span,
        )?;
        Ok(destination)
    }

    fn clone_record_array(
        &mut self,
        block: BlockId,
        value: ValueId,
        representation: crate::cc::ReprId,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let destination = self.fresh(
            self.layout
                .value_type(&ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(representation),
                }))
                .map_err(|error| layout_error(span, error))?,
        );
        self.append_instruction(
            block,
            Instruction::ArrayClone {
                destination,
                type_index: self
                    .layout
                    .repr_index(representation)
                    .map_err(|error| layout_error(span, error))?,
                value,
                span,
            },
            span,
        )?;
        Ok(destination)
    }

    fn record_array_length(
        &mut self,
        block: BlockId,
        value: ValueId,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let destination = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::ArrayLen {
                destination,
                value,
                span,
            },
            span,
        )?;
        Ok(destination)
    }
}
