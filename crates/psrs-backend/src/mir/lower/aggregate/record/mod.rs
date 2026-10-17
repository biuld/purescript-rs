//! Explicit product <-> labelled-store conversion. No reference reinterpretation.
use super::fields::StoredValue;
use super::*;
mod operations;
use crate::cc::{RecordStorage, ReprId};
use crate::mir::NumericOp;

struct RecordPlan<'a> {
    source: ReprId,
    target: ReprId,
    labels: &'a [String],
    fields: &'a [ValueConversion],
}

impl FunctionLowerer<'_> {
    pub(super) fn lower_record_conversion(
        &mut self,
        block: BlockId,
        value: ValueId,
        conversion: &ValueConversion,
        span: TextRange,
    ) -> Result<(BlockId, ValueId, ValueShape), Vec<BackendError>> {
        let storage = self
            .layout
            .representation_table()
            .record_storage()
            .ok_or_else(|| {
                aggregate_error(span, "record conversion has no registered storage schema")
            })?;
        let (plan, packing) = match conversion {
            ValueConversion::RecordPack {
                source,
                target,
                labels,
                fields,
            } => (
                RecordPlan {
                    source: *source,
                    target: *target,
                    labels,
                    fields,
                },
                true,
            ),
            ValueConversion::RecordUnpack {
                source,
                target,
                labels,
                fields,
            } => (
                RecordPlan {
                    source: *source,
                    target: *target,
                    labels,
                    fields,
                },
                false,
            ),
            _ => {
                return Err(aggregate_error(
                    span,
                    "record lowering requires a record conversion plan",
                ));
            }
        };
        if packing {
            self.pack_record(block, value, storage, plan, span)
        } else {
            self.unpack_record(block, value, storage, plan, span)
        }
    }

    fn pack_record(
        &mut self,
        block: BlockId,
        value: ValueId,
        storage: RecordStorage,
        plan: RecordPlan<'_>,
        span: TextRange,
    ) -> Result<(BlockId, ValueId, ValueShape), Vec<BackendError>> {
        let (block, fields) =
            self.convert_product_fields(block, value, plan.source, plan.fields, span)?;
        let values = fields
            .into_iter()
            .map(|field| self.restore_field(block, field, span))
            .collect::<Result<Vec<_>, _>>()?;
        let values = self.record_array(block, storage.values(), values, span)?;
        let labels = plan
            .labels
            .iter()
            .map(|label| self.record_label(block, label, span))
            .collect::<Result<Vec<_>, _>>()?;
        let labels = self.record_array(block, storage.labels(), labels, span)?;
        let values = vec![
            self.persist_field(block, labels, span)?,
            self.persist_field(block, values, span)?,
        ];
        self.build_product(block, plan.target, values, span)
    }

    fn unpack_record(
        &mut self,
        mut block: BlockId,
        value: ValueId,
        storage: RecordStorage,
        plan: RecordPlan<'_>,
        span: TextRange,
    ) -> Result<(BlockId, ValueId, ValueShape), Vec<BackendError>> {
        let arrays = self.record_arrays(block, value, storage, span)?;
        let count = i32::try_from(plan.labels.len())
            .map_err(|_| aggregate_error(span, "record has too many fields for the target"))?;
        let expected_count = self.record_integer(block, count, span)?;
        for array in &arrays {
            let value = self.restore_field(block, *array, span)?;
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
            let equal = self.fresh(ValueType::Boolean);
            self.append_instruction(
                block,
                Instruction::Primitive {
                    destination: equal,
                    op: NumericOp::I32Eq,
                    left: length,
                    right: expected_count,
                    span,
                },
                span,
            )?;
            block = self.require_record_condition(block, equal, span)?;
        }
        // Validate the complete expected row before recovering any field. A
        // shape-compatible map with different labels cannot satisfy the plan.
        for (index, label) in plan.labels.iter().enumerate() {
            let labels = self.restore_field(block, arrays[0], span)?;
            let index = self.record_integer(block, index as i32, span)?;
            let actual = self.fresh(
                self.layout
                    .value_type(&ValueShape::String)
                    .map_err(|error| layout_error(span, error))?,
            );
            self.lower_array_get(block, actual, storage.labels(), labels, index, span)?;
            let expected = self.record_label(block, label, span)?;
            let equal = self.fresh(ValueType::Boolean);
            block = self.lower_string_eq(block, equal, actual, expected, span)?;
            block = self.require_record_condition(block, equal, span)?;
        }
        let mut fields = Vec::with_capacity(plan.fields.len());
        for index in 0..plan.fields.len() {
            let array = self.restore_field(block, arrays[1], span)?;
            let index = self.record_integer(block, index as i32, span)?;
            let destination = self.fresh(
                self.layout
                    .value_type(&erased_shape())
                    .map_err(|error| layout_error(span, error))?,
            );
            self.lower_array_get(block, destination, storage.values(), array, index, span)?;
            fields.push((
                self.persist_field(block, destination, span)?,
                erased_shape(),
            ));
        }
        let (block, fields) = self.convert_stored_fields(block, fields, plan.fields, span)?;
        self.build_product(block, plan.target, fields, span)
    }

    fn record_arrays(
        &mut self,
        block: BlockId,
        value: ValueId,
        storage: RecordStorage,
        span: TextRange,
    ) -> Result<[StoredValue; 2], Vec<BackendError>> {
        let type_index = self
            .layout
            .repr_index(storage.representation())
            .map_err(|error| layout_error(span, error))?;
        let mut read = |field, repr| {
            let destination = self.fresh(
                self.layout
                    .value_type(&representation_shape(repr))
                    .map_err(|error| layout_error(span, error))?,
            );
            self.append_instruction(
                block,
                Instruction::StructGet {
                    destination,
                    type_index,
                    field,
                    value,
                    span,
                },
                span,
            )?;
            self.persist_field(block, destination, span)
        };
        Ok([read(0, storage.labels())?, read(1, storage.values())?])
    }

    fn require_record_condition(
        &mut self,
        block: BlockId,
        condition: ValueId,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let next = self.new_block(Vec::new());
        let trap = self.new_block(Vec::new());
        self.set_terminator(
            block,
            Terminator::Branch {
                condition,
                then_block: next,
                else_block: trap,
                span,
            },
            span,
        )?;
        self.set_terminator(trap, Terminator::Trap { span }, span)?;
        Ok(next)
    }

    fn record_array(
        &mut self,
        block: BlockId,
        repr: ReprId,
        elements: Vec<ValueId>,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let destination = self.fresh(
            self.layout
                .value_type(&representation_shape(repr))
                .map_err(|error| layout_error(span, error))?,
        );
        let type_index = self
            .layout
            .repr_index(repr)
            .map_err(|error| layout_error(span, error))?;
        self.append_instruction(
            block,
            Instruction::ArrayNew {
                destination,
                type_index,
                elements,
                span,
            },
            span,
        )?;
        Ok(destination)
    }

    fn record_label(
        &mut self,
        block: BlockId,
        label: &str,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let destination = self.fresh(
            self.layout
                .value_type(&ValueShape::String)
                .map_err(|error| layout_error(span, error))?,
        );
        let type_index = self.layout.string_index().ok_or_else(|| {
            aggregate_error(span, "record labels require the target string layout")
        })?;
        let data_index = self
            .literals
            .as_mut()
            .ok_or_else(|| aggregate_error(span, "record labels require the string literal pool"))?
            .intern(label);
        self.append_instruction(
            block,
            Instruction::ArrayNewData {
                destination,
                type_index,
                data_index,
                span,
            },
            span,
        )?;
        Ok(destination)
    }

    fn record_integer(
        &mut self,
        block: BlockId,
        value: i32,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let destination = self.fresh(ValueType::I32);
        self.append_instruction(
            block,
            Instruction::Constant {
                destination,
                value,
                span,
            },
            span,
        )?;
        Ok(destination)
    }
}
