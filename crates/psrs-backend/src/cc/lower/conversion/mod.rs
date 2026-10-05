use super::super::layout::{
    array_element_type, is_abstract_type, newtype_field_type, scalar_type, unquantified_type,
    user_type_id,
};
use super::super::{
    AggregateConvert, Assignment, AssignmentKind, BoxKind, RecoveryEvidence, RefShape, Reference,
    ReprId, ValueConversion, ValueId, ValueShape,
};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::TypeId;
use psrs_span::TextRange;

mod callable;
mod scalars;
mod transport;

pub(in crate::cc) struct VariantFieldConversion {
    pub(in crate::cc) variant: ReprId,
    pub(in crate::cc) tag: u32,
    pub(in crate::cc) field: u32,
    pub(in crate::cc) template_type: TypeId,
    pub(in crate::cc) target_type: TypeId,
    pub(in crate::cc) target_shape: ValueShape,
    pub(in crate::cc) stored_shape: ValueShape,
    pub(in crate::cc) span: TextRange,
}

impl FunctionLowerer<'_> {
    pub(in crate::cc) fn value_shape(
        &self,
        ty: TypeId,
        span: TextRange,
    ) -> Result<ValueShape, Vec<BackendError>> {
        scalar_type(
            self.module,
            ty,
            span,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )
    }

    pub(in crate::cc) fn typed_conversion(
        &mut self,
        source_type: TypeId,
        destination_type: TypeId,
        source_shape: ValueShape,
        destination_shape: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        self.typed_conversion_with_instantiation(
            source_type,
            destination_type,
            source_shape,
            destination_shape,
            span,
            None,
        )
    }

    pub(in crate::cc::lower) fn typed_conversion_with_instantiation(
        &mut self,
        source_type: TypeId,
        destination_type: TypeId,
        source_shape: ValueShape,
        destination_shape: ValueShape,
        span: TextRange,
        instantiation: Option<&psrs_core::Instantiation<'_>>,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        self.plan_conversion(
            source_type,
            destination_type,
            source_shape,
            destination_shape,
            span,
            instantiation,
        )
    }

    fn plan_conversion(
        &mut self,
        source_type: TypeId,
        destination_type: TypeId,
        source_shape: ValueShape,
        destination_shape: ValueShape,
        span: TextRange,
        instantiation: Option<&psrs_core::Instantiation<'_>>,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        if let Some(plan) = self.constructor_transport(
            source_type,
            destination_type,
            source_shape,
            destination_shape,
            span,
            instantiation,
        )? {
            return Ok(plan);
        }
        if source_shape == destination_shape {
            return Ok(ValueConversion::Identity);
        }
        // Newtypes use their declared field template as their storage protocol.
        // Keep the supplied physical shapes: instantiating the field here would
        // incorrectly replace erased storage with the concrete argument shape.
        let source_type = self.conversion_template(source_type, span)?;
        let destination_type = self.conversion_template(destination_type, span)?;
        if super::call::is_function_type(self.module, source_type)
            && super::call::is_function_type(self.module, destination_type)
            && matches!(
                destination_shape,
                ValueShape::Reference(Reference {
                    heap: RefShape::Closure(_),
                    ..
                })
            )
        {
            return self.function_adapter_plan(
                source_type,
                destination_type,
                source_shape,
                destination_shape,
                span,
                instantiation,
            );
        }
        if is_abstract_type(self.module, destination_type) {
            return match source_shape {
                ValueShape::Integer | ValueShape::Boolean => self
                    .box_plan(BoxKind::Integer, self.boxed_integer_type, span)
                    .map(|boxed| sequence(vec![boxed, ValueConversion::EraseReference])),
                ValueShape::Number => self
                    .box_plan(BoxKind::Number, self.boxed_number_type, span)
                    .map(|boxed| sequence(vec![boxed, ValueConversion::EraseReference])),
                // A `String` is already a GC reference in the `eq` hierarchy, so
                // it is erased and recovered by cast, not by the integer box.
                ValueShape::String | ValueShape::Reference(_) => {
                    Ok(ValueConversion::EraseReference)
                }
            };
        }
        if is_abstract_type(self.module, source_type) {
            return match destination_shape {
                ValueShape::Integer | ValueShape::Boolean => self.unbox_plan(
                    BoxKind::Integer,
                    self.boxed_integer_type,
                    destination_shape,
                    span,
                ),
                ValueShape::Number => self.unbox_plan(
                    BoxKind::Number,
                    self.boxed_number_type,
                    destination_shape,
                    span,
                ),
                ValueShape::String | ValueShape::Reference(_) => {
                    Ok(ValueConversion::RecoverReference {
                        destination: destination_shape,
                        evidence: RecoveryEvidence::TypeInstantiation,
                    })
                }
            };
        }
        // An abstract aggregate value and the concrete variant representation of
        // the same declaration are related by a reference cast (DEC-13).
        if let (
            ValueShape::Reference(source_reference),
            ValueShape::Reference(destination_reference),
        ) = (source_shape, destination_shape)
        {
            match (source_reference.heap, destination_reference.heap) {
                (RefShape::Aggregate, RefShape::Repr(_)) => {
                    return Ok(ValueConversion::RecoverReference {
                        destination: destination_shape,
                        evidence: RecoveryEvidence::TypeInstantiation,
                    });
                }
                (RefShape::Repr(_), RefShape::Aggregate) => {
                    return Ok(ValueConversion::RecoverReference {
                        destination: destination_shape,
                        evidence: RecoveryEvidence::TypeInstantiation,
                    });
                }
                // Erased storage hides an existing object shape. Callable
                // signature changes are handled above by FunctionAdapter;
                // these casts only enter or recover an erased storage slot.
                (_, RefShape::Erased) => {
                    return Ok(ValueConversion::EraseReference);
                }
                (RefShape::Erased, _) => {
                    return Ok(ValueConversion::RecoverReference {
                        destination: destination_shape,
                        evidence: RecoveryEvidence::TypeInstantiation,
                    });
                }
                _ => {}
            }
        }
        if let (Some(source_element), Some(destination_element)) = (
            array_element_type(self.module, source_type),
            array_element_type(self.module, destination_type),
        ) {
            let (Some(source_repr), Some(destination_repr)) = (
                self.array_types.get(&source_type).copied(),
                self.array_types.get(&destination_type).copied(),
            ) else {
                return Err(conversion_error(
                    span,
                    "array conversion has no canonical layout",
                ));
            };
            let source_element_shape = self.value_shape(source_element, span)?;
            let destination_element_shape = self.value_shape(destination_element, span)?;
            let element = self.plan_conversion(
                source_element,
                destination_element,
                source_element_shape,
                destination_element_shape,
                span,
                instantiation,
            )?;
            return Ok(ValueConversion::ArrayMap {
                source: source_repr,
                target: destination_repr,
                element: Box::new(element),
            });
        }
        if self.module.is_record_type(source_type) && self.module.is_record_type(destination_type) {
            let (Some(source_fields), Some(destination_fields)) = (
                self.module.record_fields(source_type),
                self.module.record_fields(destination_type),
            ) else {
                return Err(conversion_error(
                    span,
                    "record conversion requires closed rows",
                ));
            };
            let (Some(source_repr), Some(destination_repr)) = (
                self.record_types.get(&source_type).copied(),
                self.record_types.get(&destination_type).copied(),
            ) else {
                return Err(conversion_error(
                    span,
                    "record conversion has no canonical layout",
                ));
            };
            let labels = self
                .representations
                .product_labels(source_repr)
                .ok_or_else(|| conversion_error(span, "source record has no canonical labels"))?
                .to_vec();
            let target_labels = self
                .representations
                .product_labels(destination_repr)
                .ok_or_else(|| conversion_error(span, "target record has no canonical labels"))?;
            if labels.as_slice() != target_labels
                || source_fields.len() != destination_fields.len()
                || labels.len() != source_fields.len()
            {
                return Err(conversion_error(
                    span,
                    "generic record conversion requires identical closed field labels",
                ));
            }
            let mut plans = Vec::with_capacity(labels.len());
            for label in &labels {
                let source_field = source_fields
                    .iter()
                    .find(|(name, _)| name == label)
                    .map(|(_, ty)| *ty)
                    .ok_or_else(|| conversion_error(span, "source record field is missing"))?;
                let destination_field = destination_fields
                    .iter()
                    .find(|(name, _)| name == label)
                    .map(|(_, ty)| *ty)
                    .ok_or_else(|| conversion_error(span, "target record field is missing"))?;
                plans.push(self.plan_conversion(
                    source_field,
                    destination_field,
                    self.value_shape(source_field, span)?,
                    self.value_shape(destination_field, span)?,
                    span,
                    instantiation,
                )?);
            }
            return Ok(ValueConversion::ProductMap {
                source: source_repr,
                target: destination_repr,
                labels: labels.to_vec(),
                fields: plans,
            });
        }
        Err(conversion_error(
            span,
            "unsupported aggregate conversion between normalized runtime shapes",
        ))
    }

    fn conversion_template(
        &self,
        ty: TypeId,
        span: TextRange,
    ) -> Result<TypeId, Vec<BackendError>> {
        let mut ty = unquantified_type(self.module, ty);
        let mut visited = std::collections::HashSet::new();
        while let Some(id) = user_type_id(self.module, ty) {
            if !self.newtype_ids.contains(&id) {
                break;
            }
            if !visited.insert(id) {
                return Err(conversion_error(span, "recursive newtype storage template"));
            }
            ty = newtype_field_type(self.module, id)
                .ok_or_else(|| conversion_error(span, "newtype has no storage template"))?;
            ty = unquantified_type(self.module, ty);
        }
        Ok(ty)
    }

    pub(in crate::cc) fn emit_conversion(
        &mut self,
        value: ValueId,
        source: ValueShape,
        destination: ValueShape,
        plan: ValueConversion,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> ValueId {
        if matches!(plan, ValueConversion::Identity) {
            return value;
        }
        let result = self.fresh(destination);
        assignments.push(Assignment {
            destination: result,
            kind: AssignmentKind::AggregateConvert {
                destination: result,
                value,
                conversion: AggregateConvert {
                    source,
                    destination,
                    plan,
                },
            },
            span,
        });
        result
    }

    pub(in crate::cc) fn variant_field_conversion(
        &mut self,
        recovery: VariantFieldConversion,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        let VariantFieldConversion {
            variant,
            tag,
            field,
            template_type,
            target_type,
            target_shape,
            stored_shape,
            span,
        } = recovery;
        let template_shape = self.value_shape(template_type, span)?;
        if stored_shape == erased_shape()
            && is_abstract_type(self.module, template_type)
            && matches!(target_shape, ValueShape::Reference(_))
            && target_shape != erased_shape()
        {
            return Ok(ValueConversion::RecoverReference {
                destination: target_shape,
                evidence: RecoveryEvidence::ErasedVariantField {
                    variant,
                    tag,
                    field,
                    template: target_shape,
                },
            });
        }
        if stored_shape != template_shape {
            return Err(vec![BackendError::invalid_ir(
                "P8 closure conversion",
                span,
                "variant field storage does not match its normalized template",
            )]);
        }
        self.plan_conversion(
            template_type,
            target_type,
            template_shape,
            target_shape,
            span,
            None,
        )
    }
}

pub(super) fn sequence(steps: Vec<ValueConversion>) -> ValueConversion {
    match steps.len() {
        0 => ValueConversion::Identity,
        1 => steps.into_iter().next().expect("one conversion step"),
        _ => ValueConversion::Sequence(steps),
    }
}

pub(super) fn erased_shape() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

fn conversion_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}

#[cfg(test)]
mod tests;
