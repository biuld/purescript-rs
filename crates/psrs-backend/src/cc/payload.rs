//! Shared conversion planning for bare polymorphic storage slots.
//! CC and canonical ABI adapters use the same recursive owner protocols.

use super::{
    BoxKind, RecoveryEvidence, RefShape, Reference, ReprId, RepresentationTable, SignatureId,
    ValueConversion, ValueShape,
};
use crate::BackendError;
use psrs_span::TextRange;

pub(crate) trait PayloadPlanner {
    fn payload_table(&self) -> &RepresentationTable;
    fn payload_box(&self, kind: BoxKind) -> Option<ReprId>;
    fn payload_callable(
        &mut self,
        signature: SignatureId,
        entering: bool,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>>;
    fn payload_error(&self, span: TextRange, message: &'static str) -> Vec<BackendError>;

    fn erase_payload(
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
            let adapter = self.payload_callable(signature, true, span)?;
            return Ok(sequence(vec![adapter, ValueConversion::EraseReference]));
        }
        let boxed = match shape {
            ValueShape::Integer | ValueShape::Boolean => {
                self.box_plan(BoxKind::Integer, self.payload_box(BoxKind::Integer), span)?
            }
            ValueShape::Number => {
                self.box_plan(BoxKind::Number, self.payload_box(BoxKind::Number), span)?
            }
            ValueShape::String | ValueShape::Reference(_) => {
                return Ok(ValueConversion::EraseReference);
            }
        };
        Ok(sequence(vec![boxed, ValueConversion::EraseReference]))
    }

    fn recover_payload(
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
            return self.payload_callable(signature, false, span);
        }
        match shape {
            ValueShape::Integer | ValueShape::Boolean => self.unbox_plan(
                BoxKind::Integer,
                self.payload_box(BoxKind::Integer),
                shape,
                span,
            ),
            ValueShape::Number => self.unbox_plan(
                BoxKind::Number,
                self.payload_box(BoxKind::Number),
                shape,
                span,
            ),
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
            self.payload_table().representation(concrete)
        else {
            return Ok(None);
        };
        let element_shape = *element;
        let protocol = self.payload_table().representations.iter().position(|representation|
            matches!(representation, crate::cc::Representation::Array { element } if *element == erased_shape()))
            .map(|index| ReprId(index as u32))
            .ok_or_else(|| self.payload_error(span, "Array owner has no erased storage protocol"))?;
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
            .payload_table()
            .product_labels(concrete)
            .map(<[String]>::to_vec)
        else {
            return Ok(None);
        };
        let Some(crate::cc::Representation::Product { fields }) =
            self.payload_table().representation(concrete)
        else {
            return Err(self.payload_error(span, "record owner labels do not name a product"));
        };
        let fields = fields.clone();
        let protocol = super::layout::protocols::record(self.payload_table(), &labels)
            .ok_or_else(|| self.payload_error(span, "record owner has no erased field protocol"))?;
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
            .ok_or_else(|| self.payload_error(span, "erased scalar has no box representation"))
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
            .ok_or_else(|| self.payload_error(span, "erased scalar has no box representation"))
    }
}

fn erased_shape() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

fn sequence(steps: Vec<ValueConversion>) -> ValueConversion {
    match steps.len() {
        0 => ValueConversion::Identity,
        1 => steps.into_iter().next().expect("one conversion step"),
        _ => ValueConversion::Sequence(steps),
    }
}

/// The canonical ABI admits no callable values; aggregate storage still uses
/// the same owner plans as CC. Reachability and MIR lowering share this planner.
pub(crate) struct StoragePayloadPlanner<'a>(pub(crate) &'a RepresentationTable);

impl PayloadPlanner for StoragePayloadPlanner<'_> {
    fn payload_table(&self) -> &RepresentationTable {
        self.0
    }
    fn payload_box(&self, kind: crate::cc::BoxKind) -> Option<ReprId> {
        let shape = match kind {
            crate::cc::BoxKind::Integer => crate::cc::ValueShape::Integer,
            crate::cc::BoxKind::Number => crate::cc::ValueShape::Number,
        };
        self.0
            .representations
            .iter()
            .position(
                |repr| matches!(repr, crate::cc::Representation::Box { value } if *value == shape),
            )
            .map(|index| ReprId(index as u32))
    }
    fn payload_callable(
        &mut self,
        _signature: crate::cc::SignatureId,
        _entering: bool,
        span: TextRange,
    ) -> Result<crate::cc::ValueConversion, Vec<BackendError>> {
        Err(vec![BackendError::new(
            "P9 MIR lowering",
            span,
            "callable values have no canonical ABI payload conversion",
        )])
    }
    fn payload_error(&self, span: TextRange, message: &'static str) -> Vec<BackendError> {
        vec![BackendError::new("P9 MIR lowering", span, message)]
    }
}
