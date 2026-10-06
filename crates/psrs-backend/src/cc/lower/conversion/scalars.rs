use super::super::FunctionLowerer;
use super::{conversion_error, erased_shape, sequence};
use crate::{
    BackendError,
    cc::{BoxKind, RecoveryEvidence, RefShape, Reference, ReprId, ValueConversion, ValueShape},
};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(super) fn erase_payload(
        &mut self,
        shape: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        if shape == erased_shape() {
            return Ok(ValueConversion::Identity);
        }
        if let Some(plan) = self.array_payload(shape, true, span)? {
            return Ok(plan);
        }
        if let Some(plan) = self.record_payload(shape, true, span)? {
            return Ok(plan);
        }
        if let ValueShape::Reference(Reference {
            heap: RefShape::Closure(signature),
            ..
        }) = shape
        {
            let adapter = self.erase_function_slot(signature, span)?;
            return Ok(sequence(vec![adapter, ValueConversion::EraseReference]));
        }
        let boxed = match shape {
            ValueShape::Integer | ValueShape::Boolean => {
                self.box_plan(BoxKind::Integer, self.boxed_integer_type, span)?
            }
            ValueShape::Number => self.box_plan(BoxKind::Number, self.boxed_number_type, span)?,
            ValueShape::String | ValueShape::Reference(_) => {
                return Ok(ValueConversion::EraseReference);
            }
        };
        Ok(sequence(vec![boxed, ValueConversion::EraseReference]))
    }

    pub(super) fn recover_payload(
        &mut self,
        shape: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        if shape == erased_shape() {
            return Ok(ValueConversion::Identity);
        }
        if let Some(plan) = self.array_payload(shape, false, span)? {
            return Ok(plan);
        }
        if let Some(plan) = self.record_payload(shape, false, span)? {
            return Ok(plan);
        }
        if let ValueShape::Reference(Reference {
            heap: RefShape::Closure(signature),
            ..
        }) = shape
        {
            return self.recover_function_slot(signature, span);
        }
        match shape {
            ValueShape::Integer | ValueShape::Boolean => {
                self.unbox_plan(BoxKind::Integer, self.boxed_integer_type, shape, span)
            }
            ValueShape::Number => {
                self.unbox_plan(BoxKind::Number, self.boxed_number_type, shape, span)
            }
            ValueShape::String | ValueShape::Reference(_) => {
                Ok(ValueConversion::RecoverReference {
                    destination: shape,
                    evidence: RecoveryEvidence::TypeInstantiation,
                })
            }
        }
    }

    fn array_payload(
        &mut self,
        shape: ValueShape,
        entering: bool,
        span: TextRange,
    ) -> Result<Option<ValueConversion>, Vec<BackendError>> {
        let ValueShape::Reference(Reference {
            heap: RefShape::Repr(concrete),
            ..
        }) = shape
        else {
            return Ok(None);
        };
        let Some(crate::cc::Representation::Array { element }) =
            self.representations.representation(concrete)
        else {
            return Ok(None);
        };
        let element_shape = *element;
        let protocol = self.representations.representations.iter().position(|representation|
            matches!(representation, crate::cc::Representation::Array { element } if *element == erased_shape()))
            .map(|index| ReprId(index as u32))
            .ok_or_else(|| conversion_error(span, "Array owner has no erased storage protocol"))?;
        let protocol_shape = ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(protocol),
        });
        if concrete == protocol {
            return Ok(Some(if entering {
                ValueConversion::EraseReference
            } else {
                ValueConversion::RecoverReference {
                    destination: shape,
                    evidence: RecoveryEvidence::TypeInstantiation,
                }
            }));
        }
        if entering {
            let element = self.erase_payload(element_shape, span)?;
            Ok(Some(sequence(vec![
                ValueConversion::ArrayMap {
                    source: concrete,
                    target: protocol,
                    element: Box::new(element),
                },
                ValueConversion::EraseReference,
            ])))
        } else {
            let element = self.recover_payload(element_shape, span)?;
            Ok(Some(sequence(vec![
                ValueConversion::RecoverReference {
                    destination: protocol_shape,
                    evidence: RecoveryEvidence::TypeInstantiation,
                },
                ValueConversion::ArrayMap {
                    source: protocol,
                    target: concrete,
                    element: Box::new(element),
                },
            ])))
        }
    }

    fn record_payload(
        &mut self,
        shape: ValueShape,
        entering: bool,
        span: TextRange,
    ) -> Result<Option<ValueConversion>, Vec<BackendError>> {
        let ValueShape::Reference(Reference {
            heap: RefShape::Repr(concrete),
            ..
        }) = shape
        else {
            return Ok(None);
        };
        let Some(labels) = self
            .representations
            .product_labels(concrete)
            .map(<[String]>::to_vec)
        else {
            return Ok(None);
        };
        let Some(crate::cc::Representation::Product { fields }) =
            self.representations.representation(concrete)
        else {
            return Err(conversion_error(
                span,
                "record owner labels do not name a product",
            ));
        };
        let fields = fields.clone();
        let protocol =
            super::super::super::layout::protocols::record(self.representations, &labels)
                .ok_or_else(|| {
                    conversion_error(span, "record owner has no erased field protocol")
                })?;
        if concrete == protocol {
            return Ok(Some(if entering {
                ValueConversion::EraseReference
            } else {
                ValueConversion::RecoverReference {
                    destination: shape,
                    evidence: RecoveryEvidence::TypeInstantiation,
                }
            }));
        }
        let plans = fields
            .into_iter()
            .map(|field| {
                if entering {
                    self.erase_payload(field, span)
                } else {
                    self.recover_payload(field, span)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(if entering {
            sequence(vec![
                ValueConversion::ProductMap {
                    source: concrete,
                    target: protocol,
                    labels,
                    fields: plans,
                },
                ValueConversion::EraseReference,
            ])
        } else {
            let protocol_shape = ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(protocol),
            });
            sequence(vec![
                ValueConversion::RecoverReference {
                    destination: protocol_shape,
                    evidence: RecoveryEvidence::TypeInstantiation,
                },
                ValueConversion::ProductMap {
                    source: protocol,
                    target: concrete,
                    labels,
                    fields: plans,
                },
            ])
        }))
    }

    pub(super) fn box_plan(
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

    pub(super) fn unbox_plan(
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
}
