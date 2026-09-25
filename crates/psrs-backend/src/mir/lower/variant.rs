use super::{FunctionLowerer, layout_error};
use crate::BackendError;
use crate::cc::{Assignment, AssignmentKind};
use crate::mir::{BlockId, Instruction};
use crate::types::{HeapType, RefType, ValueType};

impl FunctionLowerer<'_> {
    pub(super) fn lower_variant(
        &mut self,
        assignment: &Assignment,
        current: BlockId,
    ) -> Result<(), Vec<BackendError>> {
        let span = assignment.span;
        match &assignment.kind {
            AssignmentKind::VariantNew {
                destination,
                representation,
                case,
                fields,
            } => {
                let case_type = self
                    .layout
                    .variant_index(*representation, *case)
                    .map_err(|error| layout_error(span, error))?;
                let tag = self.fresh(ValueType::I32);
                self.append_instruction(
                    current,
                    Instruction::Constant {
                        destination: tag,
                        value: *case as i32,
                        span,
                    },
                    span,
                )?;
                let arguments = std::iter::once(tag).chain(fields.iter().copied()).collect();
                self.append_instruction(
                    current,
                    Instruction::StructNew {
                        destination: *destination,
                        type_index: case_type,
                        arguments,
                        span,
                    },
                    span,
                )
            }
            AssignmentKind::VariantTag {
                destination,
                representation,
                value,
            } => {
                let supertype = self
                    .layout
                    .repr_index(*representation)
                    .map_err(|error| layout_error(span, error))?;
                let reference = RefType {
                    nullable: false,
                    heap: HeapType::Index(supertype),
                };
                let cast = self.fresh(ValueType::Ref(reference));
                self.append_instruction(
                    current,
                    Instruction::RefCast {
                        destination: cast,
                        value: *value,
                        reference,
                        span,
                    },
                    span,
                )?;
                self.append_instruction(
                    current,
                    Instruction::StructGet {
                        destination: *destination,
                        type_index: supertype,
                        field: 0,
                        value: cast,
                        span,
                    },
                    span,
                )
            }
            AssignmentKind::VariantGet {
                destination,
                representation,
                case,
                field,
                value,
            } => {
                let case_type = self
                    .layout
                    .variant_index(*representation, *case)
                    .map_err(|error| layout_error(span, error))?;
                let cast = self.fresh(ValueType::Ref(RefType {
                    nullable: false,
                    heap: HeapType::Index(case_type),
                }));
                self.append_instruction(
                    current,
                    Instruction::RefCast {
                        destination: cast,
                        value: *value,
                        reference: RefType {
                            nullable: false,
                            heap: HeapType::Index(case_type),
                        },
                        span,
                    },
                    span,
                )?;
                self.append_instruction(
                    current,
                    Instruction::StructGet {
                        destination: *destination,
                        type_index: case_type,
                        field: *field + 1,
                        value: cast,
                        span,
                    },
                    span,
                )
            }
            _ => unreachable!("variant lowering received another operation"),
        }
    }
}
