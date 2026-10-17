//! Preserve aggregate fields across nested conversions and control labels.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct StoredValue {
    value: ValueId,
    restore: Option<crate::types::RefType>,
}

impl FunctionLowerer<'_> {
    pub(super) fn persist_field(
        &mut self,
        block: BlockId,
        value: ValueId,
        span: TextRange,
    ) -> Result<StoredValue, Vec<BackendError>> {
        let ty = self
            .value_type(value)
            .ok_or_else(|| aggregate_error(span, "aggregate field has no physical type"))?;
        if let ValueType::Ref(reference) = ty
            && !reference.nullable
        {
            let nullable = crate::types::RefType {
                nullable: true,
                ..reference
            };
            let destination = self.fresh(ValueType::Ref(nullable));
            self.append_instruction(
                block,
                Instruction::RefCast {
                    destination,
                    value,
                    reference: nullable,
                    span,
                },
                span,
            )?;
            Ok(StoredValue {
                value: destination,
                restore: Some(reference),
            })
        } else {
            Ok(StoredValue {
                value,
                restore: None,
            })
        }
    }

    pub(super) fn restore_field(
        &mut self,
        block: BlockId,
        stored: StoredValue,
        span: TextRange,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(reference) = stored.restore else {
            return Ok(stored.value);
        };
        let destination = self.fresh(ValueType::Ref(reference));
        self.append_instruction(
            block,
            Instruction::RefCast {
                destination,
                value: stored.value,
                reference,
                span,
            },
            span,
        )?;
        Ok(destination)
    }

    pub(super) fn convert_product_fields(
        &mut self,
        block: BlockId,
        value: ValueId,
        source: crate::cc::ReprId,
        fields: &[ValueConversion],
        span: TextRange,
    ) -> Result<(BlockId, Vec<StoredValue>), Vec<BackendError>> {
        let type_index = self
            .layout
            .repr_index(source)
            .map_err(|error| layout_error(span, error))?;
        let mut values = Vec::with_capacity(fields.len());
        // Read all fields before a nested plan introduces branches or loops.
        // Physical String references need this just as nominal references do.
        for index in 0..fields.len() {
            let shape = self
                .layout
                .product_field(type_index, index as u32)
                .map_err(|error| layout_error(span, error))?;
            let destination = self.fresh(
                self.layout
                    .value_type(&shape)
                    .map_err(|error| layout_error(span, error))?,
            );
            self.append_instruction(
                block,
                Instruction::StructGet {
                    destination,
                    type_index,
                    field: index as u32,
                    value,
                    span,
                },
                span,
            )?;
            values.push((self.persist_field(block, destination, span)?, shape));
        }
        self.convert_stored_fields(block, values, fields, span)
    }

    pub(super) fn convert_stored_fields(
        &mut self,
        mut block: BlockId,
        values: Vec<(StoredValue, ValueShape)>,
        fields: &[ValueConversion],
        span: TextRange,
    ) -> Result<(BlockId, Vec<StoredValue>), Vec<BackendError>> {
        let mut result = Vec::with_capacity(fields.len());
        for ((value, shape), plan) in values.into_iter().zip(fields) {
            let value = self.restore_field(block, value, span)?;
            let (next, value, _) = self.lower_value_conversion(block, value, shape, plan, span)?;
            block = next;
            result.push(self.persist_field(block, value, span)?);
        }
        Ok((block, result))
    }

    pub(super) fn build_product(
        &mut self,
        block: BlockId,
        target: crate::cc::ReprId,
        values: Vec<StoredValue>,
        span: TextRange,
    ) -> Result<(BlockId, ValueId, ValueShape), Vec<BackendError>> {
        let shape = representation_shape(target);
        let destination = self.fresh(
            self.layout
                .value_type(&shape)
                .map_err(|error| layout_error(span, error))?,
        );
        let type_index = self
            .layout
            .repr_index(target)
            .map_err(|error| layout_error(span, error))?;
        let arguments = values
            .into_iter()
            .map(|value| self.restore_field(block, value, span))
            .collect::<Result<Vec<_>, _>>()?;
        self.append_instruction(
            block,
            Instruction::StructNew {
                destination,
                type_index,
                arguments,
                span,
            },
            span,
        )?;
        Ok((block, destination, shape))
    }
}
