use super::super::{Assignment, AssignmentKind, ValueId, ValueType};
use super::FunctionLowerer;
use crate::BackendError;

impl FunctionLowerer<'_> {
    pub(super) fn box_erased_value(
        &mut self,
        value: ValueId,
        span: psrs_span::TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(boxed_type) = self.boxed_i32_type else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "parameterized constructor has no erased value box layout",
            )]);
        };
        let value_type = self
            .values
            .iter()
            .find(|declaration| declaration.id == value)
            .map(|declaration| declaration.ty)
            .ok_or_else(|| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    span,
                    "erased constructor field uses an unknown value",
                )]
            })?;
        let erased = match value_type {
            ValueType::I32 | ValueType::Boolean => {
                let boxed = self.fresh(ValueType::Ref(crate::types::RefType {
                    nullable: false,
                    heap: crate::types::HeapType::Index(boxed_type),
                }));
                assignments.push(Assignment {
                    destination: boxed,
                    kind: AssignmentKind::StructNew {
                        destination: boxed,
                        type_index: boxed_type,
                        arguments: vec![value],
                    },
                    span,
                });
                boxed
            }
            ValueType::Ref(crate::types::RefType {
                nullable: false,
                heap: crate::types::HeapType::Eq,
            }) => value,
            ValueType::Ref(_) => {
                let cast = self.fresh(ValueType::Ref(crate::types::RefType {
                    nullable: false,
                    heap: crate::types::HeapType::Eq,
                }));
                assignments.push(Assignment {
                    destination: cast,
                    kind: AssignmentKind::RefCast {
                        destination: cast,
                        value,
                        reference: crate::types::RefType {
                            nullable: false,
                            heap: crate::types::HeapType::Eq,
                        },
                    },
                    span,
                });
                cast
            }
            _ => {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    span,
                    "parameterized constructor field cannot be erased yet",
                )]);
            }
        };
        let erased_type = self
            .values
            .iter()
            .find(|declaration| declaration.id == erased)
            .map(|declaration| declaration.ty)
            .expect("erased value was just allocated or already exists");
        if erased_type
            != ValueType::Ref(crate::types::RefType {
                nullable: false,
                heap: crate::types::HeapType::Eq,
            })
        {
            let cast = self.fresh(ValueType::Ref(crate::types::RefType {
                nullable: false,
                heap: crate::types::HeapType::Eq,
            }));
            assignments.push(Assignment {
                destination: cast,
                kind: AssignmentKind::RefCast {
                    destination: cast,
                    value: erased,
                    reference: crate::types::RefType {
                        nullable: false,
                        heap: crate::types::HeapType::Eq,
                    },
                },
                span,
            });
            return Ok(cast);
        }
        Ok(erased)
    }
}
