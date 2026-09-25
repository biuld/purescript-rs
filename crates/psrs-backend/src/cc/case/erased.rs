use super::super::layout::erased_field_recovery_family;
use super::super::lower::FunctionLowerer;
use super::super::{Assignment, AssignmentKind, RefShape, Reference, ValueId, ValueShape};
use super::case_error;
use crate::BackendError;
use psrs_core::TypeId;
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(super) fn adapt_projected_field(
        &mut self,
        value: ValueId,
        stored: ValueShape,
        expected: ValueShape,
        target_type: TypeId,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        if stored == expected {
            return Ok(value);
        }
        if !matches!(
            stored,
            ValueShape::Reference(Reference {
                heap: RefShape::Erased,
                ..
            })
        ) {
            return Err(case_error(
                span,
                "pattern field runtime representation does not match its target type",
            ));
        }
        if let Some(family) = erased_field_recovery_family(
            self.module,
            target_type,
            stored,
            expected,
            self.array_types,
            self.record_types,
        ) {
            return Err(case_error(
                span,
                format!(
                    "unsupported generic {family} field recovery: its nominal runtime layout depends on a type variable"
                ),
            ));
        }
        match expected {
            ValueShape::Integer | ValueShape::Boolean => {
                let Some(boxed_type) = self.boxed_integer_type else {
                    return Err(case_error(span, "erased value box layout is missing"));
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
                let value = self.fresh(expected);
                assignments.push(Assignment {
                    destination: value,
                    kind: AssignmentKind::ProductGet {
                        destination: value,
                        representation: boxed_type,
                        field: 0,
                        value: concrete_box,
                    },
                    span,
                });
                Ok(value)
            }
            ValueShape::Number => {
                let Some(boxed_type) = self.boxed_number_type else {
                    return Err(case_error(
                        span,
                        "erased number box representation is missing",
                    ));
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
                let value = self.fresh(expected);
                assignments.push(Assignment {
                    destination: value,
                    kind: AssignmentKind::ProductGet {
                        destination: value,
                        representation: boxed_type,
                        field: 0,
                        value: concrete_box,
                    },
                    span,
                });
                Ok(value)
            }
            ValueShape::Reference(reference) => {
                let projected = value;
                let value = self.fresh(ValueShape::Reference(reference));
                assignments.push(Assignment {
                    destination: value,
                    kind: AssignmentKind::RepresentationCast {
                        destination: value,
                        value: projected,
                        reference,
                    },
                    span,
                });
                Ok(value)
            }
        }
    }
}
