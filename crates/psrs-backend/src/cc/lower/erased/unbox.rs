use super::super::super::{
    AggregateConvert, Assignment, AssignmentKind, RecoveryEvidence, RefShape, Reference, UnaryOp,
    ValueConversion, ValueId, ValueShape,
};
use super::super::FunctionLowerer;
use super::erased_reference_type;
use crate::BackendError;

impl FunctionLowerer<'_> {
    /// Recovers a concrete value from its erased representation. Reverses
    /// exactly the box chosen for `expected`, and is an identity for the erased
    /// shape itself.
    pub(in crate::cc::lower) fn unbox_erased_value(
        &mut self,
        value: ValueId,
        expected: ValueShape,
        span: psrs_span::TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        match expected {
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            }) => Ok(value),
            // A `String` is a GC reference in the `eq` hierarchy, so it is
            // recovered by a cast to `(ref $string)`, not by the integer box.
            ValueShape::String => {
                let result = self.fresh(ValueShape::String);
                assignments.push(Assignment {
                    destination: result,
                    kind: AssignmentKind::AggregateConvert {
                        destination: result,
                        value,
                        conversion: AggregateConvert {
                            source: erased_reference_type(),
                            destination: ValueShape::String,
                            plan: ValueConversion::RecoverReference {
                                destination: ValueShape::String,
                                evidence: RecoveryEvidence::TypeInstantiation,
                            },
                        },
                    },
                    span,
                });
                Ok(result)
            }
            ValueShape::Integer | ValueShape::Boolean => {
                let Some(boxed_type) = self.boxed_integer_type else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        span,
                        "polymorphic result has no integer box representation",
                    )]);
                };
                let concrete_box = self.fresh(ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(boxed_type),
                }));
                assignments.push(Assignment {
                    destination: concrete_box,
                    kind: AssignmentKind::RepresentationCast {
                        destination: concrete_box,
                        value,
                        reference: Reference {
                            nullable: false,
                            heap: RefShape::Repr(boxed_type),
                        },
                    },
                    span,
                });
                // The box stores Wasm i32; a Boolean destination is recovered
                // through an explicit `IntToBoolean` conversion so the product
                // projection keeps the box field's integer shape.
                let boxed_destination = if expected == ValueShape::Boolean {
                    self.fresh(ValueShape::Integer)
                } else {
                    self.fresh(expected)
                };
                assignments.push(Assignment {
                    destination: boxed_destination,
                    kind: AssignmentKind::ProductGet {
                        destination: boxed_destination,
                        representation: boxed_type,
                        field: 0,
                        value: concrete_box,
                    },
                    span,
                });
                if expected == ValueShape::Boolean {
                    let result = self.fresh(ValueShape::Boolean);
                    assignments.push(Assignment {
                        destination: result,
                        kind: AssignmentKind::Unary {
                            op: UnaryOp::IntToBoolean,
                            value: boxed_destination,
                        },
                        span,
                    });
                    Ok(result)
                } else {
                    Ok(boxed_destination)
                }
            }
            ValueShape::Number => {
                let Some(boxed_type) = self.boxed_number_type else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        span,
                        "polymorphic result has no number box representation",
                    )]);
                };
                let concrete_box = self.fresh(ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(boxed_type),
                }));
                assignments.push(Assignment {
                    destination: concrete_box,
                    kind: AssignmentKind::RepresentationCast {
                        destination: concrete_box,
                        value,
                        reference: Reference {
                            nullable: false,
                            heap: RefShape::Repr(boxed_type),
                        },
                    },
                    span,
                });
                let result = self.fresh(ValueShape::Number);
                assignments.push(Assignment {
                    destination: result,
                    kind: AssignmentKind::ProductGet {
                        destination: result,
                        representation: boxed_type,
                        field: 0,
                        value: concrete_box,
                    },
                    span,
                });
                Ok(result)
            }
            ValueShape::Reference(reference) => {
                let result = self.fresh(ValueShape::Reference(reference));
                assignments.push(Assignment {
                    destination: result,
                    kind: AssignmentKind::RepresentationCast {
                        destination: result,
                        value,
                        reference,
                    },
                    span,
                });
                Ok(result)
            }
        }
    }
}
