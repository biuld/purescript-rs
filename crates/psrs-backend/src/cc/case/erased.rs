use super::super::layout::scalar_type;
use super::super::lower::FunctionLowerer;
use super::super::{Assignment, AssignmentKind, RefShape, Reference, ReprId, ValueId, ValueShape};
use super::case_error;
use crate::BackendError;
use psrs_span::TextRange;

pub(super) struct VariantField {
    pub(super) representation: ReprId,
    pub(super) case: u32,
    pub(super) field: u32,
}

impl FunctionLowerer<'_> {
    pub(super) fn lower_erased_field(
        &mut self,
        expected_type: psrs_core::TypeId,
        constructor: ValueId,
        selector: VariantField,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let boxed_value = self.fresh(ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        }));
        assignments.push(Assignment {
            destination: boxed_value,
            kind: AssignmentKind::VariantGet {
                destination: boxed_value,
                representation: selector.representation,
                case: selector.case,
                field: selector.field,
                value: constructor,
            },
            span,
        });
        let expected = scalar_type(
            self.module,
            expected_type,
            span,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )?;
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
                        value: boxed_value,
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
                        value: boxed_value,
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
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            }) => Ok(boxed_value),
            ValueShape::Reference(reference) => {
                let value = self.fresh(ValueShape::Reference(reference));
                assignments.push(Assignment {
                    destination: value,
                    kind: AssignmentKind::RepresentationCast {
                        destination: value,
                        value: boxed_value,
                        reference,
                    },
                    span,
                });
                Ok(value)
            }
        }
    }
}
