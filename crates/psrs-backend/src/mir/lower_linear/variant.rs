use super::{LinearFunctionLowerer, layout_error};
use crate::BackendError;
use crate::cc::{Assignment, AssignmentKind};
use crate::mir::{BlockId, Instruction};
use crate::types::ValueType;

impl LinearFunctionLowerer<'_> {
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
                let bytes = self
                    .layout
                    .representation_size(*representation)
                    .map_err(|error| layout_error(span, error))?;
                let planned = self
                    .layout
                    .variant_fields(*representation, *case)
                    .map_err(|error| layout_error(span, error))?
                    .to_vec();
                self.append(
                    current,
                    Instruction::LinearAlloc {
                        destination: *destination,
                        bytes,
                        alignment: self
                            .layout
                            .representation_alignment(*representation)
                            .map_err(|error| layout_error(span, error))?,
                        span,
                    },
                    span,
                )?;
                let tag = self.fresh(ValueType::I32);
                self.append(
                    current,
                    Instruction::Constant {
                        destination: tag,
                        value: *case as i32,
                        span,
                    },
                    span,
                )?;
                self.append(
                    current,
                    Instruction::LinearStore {
                        memory: crate::types::MemoryId(0),
                        alignment: 4,
                        address: *destination,
                        value: tag,
                        offset: 0,
                        object_bytes: bytes,
                        ty: ValueType::I32,
                        span,
                    },
                    span,
                )?;
                for (field, value) in planned.iter().zip(fields) {
                    self.append(
                        current,
                        Instruction::LinearStore {
                            memory: crate::types::MemoryId(0),
                            alignment: super::LinearMemoryLayout::value_alignment(field.value),
                            address: *destination,
                            value: *value,
                            offset: field.offset,
                            object_bytes: bytes,
                            ty: super::LinearMemoryLayout::value_type(field.value),
                            span,
                        },
                        span,
                    )?;
                }
                Ok(())
            }
            AssignmentKind::VariantTag {
                destination,
                representation,
                value,
            } => {
                let object_bytes = self
                    .layout
                    .representation_size(*representation)
                    .map_err(|error| layout_error(span, error))?;
                self.append(
                    current,
                    Instruction::LinearLoad {
                        memory: crate::types::MemoryId(0),
                        alignment: 4,
                        destination: *destination,
                        address: *value,
                        offset: 0,
                        object_bytes,
                        ty: ValueType::I32,
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
                let object_bytes = self
                    .layout
                    .representation_size(*representation)
                    .map_err(|error| layout_error(span, error))?;
                let planned = self
                    .layout
                    .variant_fields(*representation, *case)
                    .map_err(|error| layout_error(span, error))?;
                let selected = planned
                    .get(*field as usize)
                    .ok_or_else(|| layout_error(span, super::LayoutError::UnknownField))?;
                self.append(
                    current,
                    Instruction::LinearLoad {
                        memory: crate::types::MemoryId(0),
                        alignment: super::LinearMemoryLayout::value_alignment(selected.value),
                        destination: *destination,
                        address: *value,
                        offset: selected.offset,
                        object_bytes,
                        ty: super::LinearMemoryLayout::value_type(selected.value),
                        span,
                    },
                    span,
                )
            }
            _ => unreachable!("variant lowering received another operation"),
        }
    }
}
