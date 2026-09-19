use super::super::layout::scalar_type;
use super::super::lower::FunctionLowerer;
use super::super::{Assignment, AssignmentKind, ValueId, ValueType};
use super::case_error;
use crate::BackendError;
use crate::types::{HeapType, RefType};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(super) fn lower_erased_field(
        &mut self,
        expected_type: psrs_core::TypeId,
        constructor: ValueId,
        constructor_type: u32,
        field: u32,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let boxed_value = self.fresh(ValueType::Ref(RefType {
            nullable: false,
            heap: HeapType::Eq,
        }));
        assignments.push(Assignment {
            destination: boxed_value,
            kind: AssignmentKind::StructGet {
                destination: boxed_value,
                type_index: constructor_type,
                field,
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
            ValueType::I32 | ValueType::Boolean => {
                let Some(boxed_type) = self.boxed_i32_type else {
                    return Err(case_error(span, "erased value box layout is missing"));
                };
                let concrete_box = self.fresh(ValueType::Ref(RefType {
                    nullable: false,
                    heap: HeapType::Index(boxed_type),
                }));
                assignments.push(Assignment {
                    destination: concrete_box,
                    kind: AssignmentKind::RefCast {
                        destination: concrete_box,
                        value: boxed_value,
                        reference: RefType {
                            nullable: false,
                            heap: HeapType::Index(boxed_type),
                        },
                    },
                    span,
                });
                let value = self.fresh(expected);
                assignments.push(Assignment {
                    destination: value,
                    kind: AssignmentKind::StructGet {
                        destination: value,
                        type_index: boxed_type,
                        field: 0,
                        value: concrete_box,
                    },
                    span,
                });
                Ok(value)
            }
            ValueType::F64 => {
                let Some(boxed_type) = self.boxed_f64_type else {
                    return Err(case_error(span, "erased f64 value box layout is missing"));
                };
                let concrete_box = self.fresh(ValueType::Ref(RefType {
                    nullable: false,
                    heap: HeapType::Index(boxed_type),
                }));
                assignments.push(Assignment {
                    destination: concrete_box,
                    kind: AssignmentKind::RefCast {
                        destination: concrete_box,
                        value: boxed_value,
                        reference: RefType {
                            nullable: false,
                            heap: HeapType::Index(boxed_type),
                        },
                    },
                    span,
                });
                let value = self.fresh(expected);
                assignments.push(Assignment {
                    destination: value,
                    kind: AssignmentKind::StructGet {
                        destination: value,
                        type_index: boxed_type,
                        field: 0,
                        value: concrete_box,
                    },
                    span,
                });
                Ok(value)
            }
            ValueType::Ref(RefType {
                nullable: false,
                heap: HeapType::Eq,
            }) => Ok(boxed_value),
            ValueType::Ref(reference) => {
                let value = self.fresh(ValueType::Ref(reference));
                assignments.push(Assignment {
                    destination: value,
                    kind: AssignmentKind::RefCast {
                        destination: value,
                        value: boxed_value,
                        reference,
                    },
                    span,
                });
                Ok(value)
            }
            _ => Err(case_error(
                span,
                "erased field cannot be unboxed to this runtime type yet",
            )),
        }
    }
}
