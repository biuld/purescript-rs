use super::*;
use crate::cc::{AggregateConvert, RefShape, Reference, ValueConversion, ValueShape};
use crate::types::{RefType, ValueType};

mod array;
mod fields;
mod record;

impl FunctionLowerer<'_> {
    pub(in crate::mir) fn lower_aggregate_convert(
        &mut self,
        block: BlockId,
        value: ValueId,
        destination: ValueId,
        conversion: &AggregateConvert,
        span: TextRange,
    ) -> Result<BlockId, Vec<BackendError>> {
        if let Some(function) = self
            .conversion_helpers
            .as_mut()
            .and_then(|helpers| helpers.intern(conversion, span))
        {
            self.append_instruction(
                block,
                Instruction::Call {
                    contract: None,
                    destination,
                    function,
                    arguments: vec![value],
                    span,
                },
                span,
            )?;
            return Ok(block);
        }
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

    pub(in crate::mir) fn lower_value_conversion(
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
            ValueConversion::FunctionAdapter {
                function,
                source: expected,
                destination,
            } => {
                if source != *expected {
                    return Err(aggregate_error(
                        span,
                        "function adapter source shape mismatch",
                    ));
                }
                let result = self.fresh(
                    self.layout
                        .value_type(destination)
                        .map_err(|error| layout_error(span, error))?,
                );
                self.append_instruction(
                    block,
                    Instruction::Call {
                        contract: None,
                        destination: result,
                        function: *function,
                        arguments: vec![value],
                        span,
                    },
                    span,
                )?;
                Ok((block, result, *destination))
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
            ValueConversion::RecordPack { .. } | ValueConversion::RecordUnpack { .. } => {
                self.lower_record_conversion(block, value, plan, span)
            }
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
        block: BlockId,
        value: ValueId,
        source: crate::cc::ReprId,
        target: crate::cc::ReprId,
        fields: &[ValueConversion],
        span: TextRange,
    ) -> Result<(BlockId, ValueId, ValueShape), Vec<BackendError>> {
        let (block, values) = self.convert_product_fields(block, value, source, fields, span)?;
        self.build_product(block, target, values, span)
    }
}

pub(super) fn representation_shape(id: crate::cc::ReprId) -> ValueShape {
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

pub(super) fn nullable_reference_shape(shape: ValueShape) -> ValueShape {
    let ValueShape::Reference(mut reference) = shape else {
        unreachable!("aggregate representation must be a reference")
    };
    reference.nullable = true;
    ValueShape::Reference(reference)
}

pub(super) fn reference_type(
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
