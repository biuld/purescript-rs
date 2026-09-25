use super::super::layout::{array_element_type, scalar_type};
use super::super::{
    AggregateConvert, Assignment, AssignmentKind, BoxKind, RecoveryEvidence, RefShape, Reference,
    ReprId, ValueConversion, ValueId, ValueShape,
};
use super::FunctionLowerer;
use crate::BackendError;
use psrs_core::{Type, TypeId};
use psrs_span::TextRange;

pub(in crate::cc) struct ErasedFieldRecovery {
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
        &self,
        source_type: TypeId,
        destination_type: TypeId,
        source_shape: ValueShape,
        destination_shape: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        self.plan_conversion(
            source_type,
            destination_type,
            source_shape,
            destination_shape,
            span,
        )
    }

    fn plan_conversion(
        &self,
        source_type: TypeId,
        destination_type: TypeId,
        source_shape: ValueShape,
        destination_shape: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        if source_shape == destination_shape {
            return Ok(ValueConversion::Identity);
        }
        let source = self.module.types.get(source_type.0 as usize);
        let destination = self.module.types.get(destination_type.0 as usize);
        if matches!(destination, Some(Type::Variable(_))) {
            return match source_shape {
                ValueShape::Integer | ValueShape::Boolean => self
                    .box_plan(BoxKind::Integer, self.boxed_integer_type, span)
                    .map(|boxed| sequence(vec![boxed, ValueConversion::EraseReference])),
                ValueShape::Number => self
                    .box_plan(BoxKind::Number, self.boxed_number_type, span)
                    .map(|boxed| sequence(vec![boxed, ValueConversion::EraseReference])),
                ValueShape::Reference(_) => Ok(ValueConversion::EraseReference),
            };
        }
        if matches!(source, Some(Type::Variable(_))) {
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
                ValueShape::Reference(_) => Ok(ValueConversion::RecoverReference {
                    destination: destination_shape,
                    evidence: RecoveryEvidence::TypeInstantiation,
                }),
            };
        }
        if let (Some(source_element), Some(destination_element)) = (
            array_element_type(self.module, source_type),
            array_element_type(self.module, destination_type),
        ) {
            let (Some(source_repr), Some(destination_repr)) = (
                self.array_types.get(&source_type),
                self.array_types.get(&destination_type),
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
            )?;
            return Ok(ValueConversion::ArrayMap {
                source: *source_repr,
                target: *destination_repr,
                element: Box::new(element),
            });
        }
        if let (Some(Type::Record(source_fields)), Some(Type::Record(destination_fields))) =
            (source, destination)
        {
            let (Some(source_repr), Some(destination_repr)) = (
                self.record_types.get(&source_type),
                self.record_types.get(&destination_type),
            ) else {
                return Err(conversion_error(
                    span,
                    "record conversion has no canonical layout",
                ));
            };
            let labels = self
                .representations
                .product_labels(*source_repr)
                .ok_or_else(|| conversion_error(span, "source record has no canonical labels"))?;
            let target_labels = self
                .representations
                .product_labels(*destination_repr)
                .ok_or_else(|| conversion_error(span, "target record has no canonical labels"))?;
            if labels != target_labels
                || source_fields.len() != destination_fields.len()
                || labels.len() != source_fields.len()
            {
                return Err(conversion_error(
                    span,
                    "generic record conversion requires identical closed field labels",
                ));
            }
            let mut plans = Vec::with_capacity(labels.len());
            for label in labels {
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
                )?);
            }
            return Ok(ValueConversion::ProductMap {
                source: *source_repr,
                target: *destination_repr,
                labels: labels.to_vec(),
                fields: plans,
            });
        }
        Err(conversion_error(
            span,
            "unsupported aggregate conversion between normalized runtime shapes",
        ))
    }

    fn box_plan(
        &self,
        kind: BoxKind,
        representation: Option<ReprId>,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        representation
            .map(|representation| ValueConversion::BoxScalar {
                kind,
                representation,
            })
            .ok_or_else(|| conversion_error(span, "erased scalar has no box representation"))
    }

    fn unbox_plan(
        &self,
        kind: BoxKind,
        representation: Option<ReprId>,
        destination: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        representation
            .map(|representation| ValueConversion::UnboxScalar {
                kind,
                representation,
                destination,
            })
            .ok_or_else(|| conversion_error(span, "erased scalar has no box representation"))
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

    pub(in crate::cc) fn erased_field_recovery(
        &self,
        recovery: ErasedFieldRecovery,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        let ErasedFieldRecovery {
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
        let mut steps = Vec::new();
        if stored_shape == erased_shape()
            && matches!(
                template_type_of(self.module, template_type),
                Some(Type::Variable(_))
            )
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
            if stored_shape != erased_shape() || !matches!(template_shape, ValueShape::Reference(_))
            {
                return Err(conversion_error(
                    span,
                    "erased variant recovery has incompatible shapes",
                ));
            }
            steps.push(ValueConversion::RecoverReference {
                destination: template_shape,
                evidence: RecoveryEvidence::ErasedVariantField {
                    variant,
                    tag,
                    field,
                    template: template_shape,
                },
            });
        }
        let specialize = self.plan_conversion(
            template_type,
            target_type,
            template_shape,
            target_shape,
            span,
        )?;
        if !matches!(specialize, ValueConversion::Identity) {
            steps.push(specialize);
        }
        Ok(sequence(steps))
    }
}

fn template_type_of(module: &psrs_core::Module, ty: TypeId) -> Option<&Type> {
    module.types.get(ty.0 as usize)
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
