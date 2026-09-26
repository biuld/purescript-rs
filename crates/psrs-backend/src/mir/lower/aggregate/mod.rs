use super::*;
use crate::cc::{AggregateConvert, RefShape, Reference, ValueConversion, ValueShape};
use crate::types::{RefType, ValueType};

mod array;

impl FunctionLowerer<'_> {
    pub(super) fn lower_aggregate_convert(
        &mut self,
        block: BlockId,
        value: ValueId,
        destination: ValueId,
        conversion: &AggregateConvert,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        let (block, result, shape) =
            self.lower_value_conversion(block, value, conversion.source, &conversion.plan, span)?;
        if shape != conversion.destination {
            return Err(aggregate_error(
                span,
                "aggregate conversion produced an unexpected shape",
            ));
        }
        self.append_instruction(
            block,
            Instruction::Copy {
                destination,
                value: result,
                span,
            },
            span,
        )?;
        Ok(block)
    }

    fn lower_value_conversion(
        &mut self,
        block: BlockId,
        value: ValueId,
        source: ValueShape,
        plan: &ValueConversion,
        span: TextRange,
    ) -> Result<(BlockId, ValueId, ValueShape), Vec<BackendError>> {
        match plan {
            ValueConversion::Identity => Ok((block, value, source)),
            ValueConversion::BoxScalar { representation, .. } => {
                let target = representation_shape(*representation);
                let result = self.fresh(
                    self.layout
                        .value_type(&target)
                        .map_err(|error| layout_error(span, error))?,
                );
                let type_index = self
                    .layout
                    .repr_index(*representation)
                    .map_err(|error| layout_error(span, error))?;
                let boxed_value = if source == ValueShape::Boolean {
                    let converted = self.fresh(ValueType::I32);
                    self.append_instruction(
                        block,
                        Instruction::UnaryPrimitive {
                            destination: converted,
                            op: super::super::UnaryOp::BoolToI32,
                            value,
                            span,
                        },
                        span,
                    )?;
                    converted
                } else {
                    value
                };
                self.append_instruction(
                    block,
                    Instruction::StructNew {
                        destination: result,
                        type_index,
                        arguments: vec![boxed_value],
                        span,
                    },
                    span,
                )?;
                Ok((block, result, target))
            }
            ValueConversion::UnboxScalar {
                representation,
                destination,
                ..
            } => {
                let boxed_shape = representation_shape(*representation);
                let boxed = self.fresh(
                    self.layout
                        .value_type(&boxed_shape)
                        .map_err(|error| layout_error(span, error))?,
                );
                let reference = reference_type(&boxed_shape, self.layout, span)?;
                self.append_instruction(
                    block,
                    Instruction::RefCast {
                        destination: boxed,
                        value,
                        reference,
                        span,
                    },
                    span,
                )?;
                let type_index = self
                    .layout
                    .repr_index(*representation)
                    .map_err(|error| layout_error(span, error))?;
                let result = self.fresh(
                    self.layout
                        .value_type(destination)
                        .map_err(|error| layout_error(span, error))?,
                );
                let get_destination = if *destination == ValueShape::Boolean {
                    self.fresh(ValueType::I32)
                } else {
                    result
                };
                self.append_instruction(
                    block,
                    Instruction::StructGet {
                        destination: get_destination,
                        type_index,
                        field: 0,
                        value: boxed,
                        span,
                    },
                    span,
                )?;
                if *destination == ValueShape::Boolean {
                    self.append_instruction(
                        block,
                        Instruction::UnaryPrimitive {
                            destination: result,
                            op: super::super::UnaryOp::I32ToBool,
                            value: get_destination,
                            span,
                        },
                        span,
                    )?;
                }
                Ok((block, result, *destination))
            }
            ValueConversion::EraseReference => {
                let destination = erased_shape();
                self.lower_reference_cast(block, value, destination, span)
            }
            ValueConversion::RecoverReference { destination, .. } => {
                self.lower_reference_cast(block, value, *destination, span)
            }
            ValueConversion::Sequence(plans) => {
                let mut current = block;
                let mut current_value = value;
                let mut current_shape = source;
                for step in plans {
                    (current, current_value, current_shape) = self.lower_value_conversion(
                        current,
                        current_value,
                        current_shape,
                        step,
                        span,
                    )?;
                }
                Ok((current, current_value, current_shape))
            }
            ValueConversion::ArrayMap {
                source: source_repr,
                target,
                element,
            } => self.lower_array_map(block, value, *source_repr, *target, element, span),
            ValueConversion::ProductMap {
                source: source_repr,
                target,
                fields,
                ..
            } => self.lower_product_map(block, value, *source_repr, *target, fields, span),
        }
    }

    fn lower_reference_cast(
        &mut self,
        block: BlockId,
        value: ValueId,
        destination: ValueShape,
        span: TextRange,
    ) -> Result<(BlockId, ValueId, ValueShape), Vec<BackendError>> {
        let result = self.fresh(
            self.layout
                .value_type(&destination)
                .map_err(|error| layout_error(span, error))?,
        );
        let reference = reference_type(&destination, self.layout, span)?;
        self.append_instruction(
            block,
            Instruction::RefCast {
                destination: result,
                value,
                reference,
                span,
            },
            span,
        )?;
        Ok((block, result, destination))
    }

    fn lower_product_map(
        &mut self,
        mut block: BlockId,
        value: ValueId,
        source: crate::cc::ReprId,
        target: crate::cc::ReprId,
        fields: &[ValueConversion],
        span: TextRange,
    ) -> Result<(BlockId, ValueId, ValueShape), Vec<BackendError>> {
        let source_index = self
            .layout
            .repr_index(source)
            .map_err(|error| layout_error(span, error))?;
        let target_index = self
            .layout
            .repr_index(target)
            .map_err(|error| layout_error(span, error))?;
        // Read every source field before a nested ArrayMap introduces loop
        // labels. Non-null product references are scoped by the structured
        // encoder, so they must not be loaded after leaving their defining
        // basic-block region.
        let mut source_fields = Vec::with_capacity(fields.len());
        for index in 0..fields.len() {
            let source_shape = self
                .layout
                .product_field(source_index, index as u32)
                .map_err(|error| layout_error(span, error))?;
            let field_value = self.fresh(
                self.layout
                    .value_type(&source_shape)
                    .map_err(|error| layout_error(span, error))?,
            );
            self.append_instruction(
                block,
                Instruction::StructGet {
                    destination: field_value,
                    type_index: source_index,
                    field: index as u32,
                    value,
                    span,
                },
                span,
            )?;
            let (field_value, is_nullable) = if matches!(source_shape, ValueShape::Reference(reference) if !reference.nullable)
            {
                let nullable_shape = nullable_reference_shape(source_shape);
                let nullable = self.fresh(
                    self.layout
                        .value_type(&nullable_shape)
                        .map_err(|error| layout_error(span, error))?,
                );
                self.append_instruction(
                    block,
                    Instruction::RefCast {
                        destination: nullable,
                        value: field_value,
                        reference: reference_type(&nullable_shape, self.layout, span)?,
                        span,
                    },
                    span,
                )?;
                (nullable, true)
            } else {
                (field_value, false)
            };
            source_fields.push((field_value, source_shape, is_nullable));
        }

        let mut values = Vec::with_capacity(fields.len());
        for ((field_value, source_shape, is_nullable), plan) in
            source_fields.into_iter().zip(fields)
        {
            let field_value = if is_nullable {
                let restored = self.fresh(
                    self.layout
                        .value_type(&source_shape)
                        .map_err(|error| layout_error(span, error))?,
                );
                self.append_instruction(
                    block,
                    Instruction::RefCast {
                        destination: restored,
                        value: field_value,
                        reference: reference_type(&source_shape, self.layout, span)?,
                        span,
                    },
                    span,
                )?;
                restored
            } else {
                field_value
            };
            let (next, converted, converted_shape) =
                self.lower_value_conversion(block, field_value, source_shape, plan, span)?;
            block = next;
            let persistent = if matches!(converted_shape, ValueShape::Reference(reference) if !reference.nullable)
            {
                let nullable_shape = nullable_reference_shape(converted_shape);
                let nullable = self.fresh(
                    self.layout
                        .value_type(&nullable_shape)
                        .map_err(|error| layout_error(span, error))?,
                );
                self.append_instruction(
                    block,
                    Instruction::RefCast {
                        destination: nullable,
                        value: converted,
                        reference: reference_type(&nullable_shape, self.layout, span)?,
                        span,
                    },
                    span,
                )?;
                (nullable, Some(converted_shape))
            } else {
                (converted, None)
            };
            values.push(persistent);
        }
        let destination_shape = representation_shape(target);
        let destination = self.fresh(
            self.layout
                .value_type(&destination_shape)
                .map_err(|error| layout_error(span, error))?,
        );
        let arguments = values
            .into_iter()
            .map(|(value, shape)| {
                let Some(shape) = shape else {
                    return Ok(value);
                };
                let restored = self.fresh(
                    self.layout
                        .value_type(&shape)
                        .map_err(|error| layout_error(span, error))?,
                );
                self.append_instruction(
                    block,
                    Instruction::RefCast {
                        destination: restored,
                        value,
                        reference: reference_type(&shape, self.layout, span)?,
                        span,
                    },
                    span,
                )?;
                Ok(restored)
            })
            .collect::<Result<Vec<_>, Vec<BackendError>>>()?;
        self.append_instruction(
            block,
            Instruction::StructNew {
                destination,
                type_index: target_index,
                arguments,
                span,
            },
            span,
        )?;
        Ok((block, destination, destination_shape))
    }
}

fn representation_shape(id: crate::cc::ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(id),
    })
}

fn erased_shape() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

fn nullable_reference_shape(shape: ValueShape) -> ValueShape {
    let ValueShape::Reference(mut reference) = shape else {
        unreachable!("aggregate representation must be a reference")
    };
    reference.nullable = true;
    ValueShape::Reference(reference)
}

fn reference_type(
    shape: &ValueShape,
    layout: &crate::mir::layout::PlannedLayout,
    span: TextRange,
) -> Result<RefType, Vec<BackendError>> {
    // A `String`'s concrete type is its GC array reference, so resolve the
    // target through the layout rather than only through `RefShape`.
    match layout
        .value_type(shape)
        .map_err(|error| layout_error(span, error))?
    {
        ValueType::Ref(reference) => Ok(reference),
        _ => Err(aggregate_error(
            span,
            "reference conversion target is not a reference",
        )),
    }
}

fn aggregate_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P9 MIR lowering", span, message)]
}
