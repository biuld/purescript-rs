//! Dispatch of the array and string/byte assignments to their lowerings.
//!
//! These all name an array representation, so their representation lookup lives
//! together. `arrayAppend` and the byte conversions replace the current block;
//! the rest append one instruction.

use super::{BlockId, FunctionLowerer, layout_error};
use crate::BackendError;
use crate::cc::{Assignment, AssignmentKind};
use crate::mir::instruction::Instruction;

impl FunctionLowerer<'_> {
    pub(super) fn lower_array_assignment(
        &mut self,
        mut current: BlockId,
        assignment: &Assignment,
    ) -> Result<BlockId, Vec<BackendError>> {
        match &assignment.kind {
            AssignmentKind::ArrayNew {
                destination,
                representation,
                elements,
            } => self.append_instruction(
                current,
                Instruction::ArrayNew {
                    destination: *destination,
                    type_index: self
                        .layout
                        .repr_index(*representation)
                        .map_err(|error| layout_error(assignment.span, error))?,
                    elements: elements.clone(),
                    span: assignment.span,
                },
                assignment.span,
            )?,
            AssignmentKind::ArrayLen { destination, value } => self.append_instruction(
                current,
                Instruction::ArrayLen {
                    destination: *destination,
                    value: *value,
                    span: assignment.span,
                },
                assignment.span,
            )?,
            AssignmentKind::ArrayAppend {
                destination,
                representation,
                left,
                right,
            } => {
                current = self.lower_array_append(
                    current,
                    *destination,
                    *representation,
                    *left,
                    *right,
                    assignment.span,
                )?;
            }
            AssignmentKind::StringToBytes {
                destination,
                representation,
                value,
            } => {
                current = self.lower_string_to_bytes(
                    current,
                    *destination,
                    *representation,
                    *value,
                    assignment.span,
                )?;
            }
            AssignmentKind::BytesToString {
                destination,
                representation,
                value,
            } => {
                current = self.lower_bytes_to_string(
                    current,
                    *destination,
                    *representation,
                    *value,
                    assignment.span,
                )?;
            }
            AssignmentKind::ArrayGet {
                destination,
                representation,
                value,
                index,
            } => self.lower_array_get(
                current,
                *destination,
                *representation,
                *value,
                *index,
                assignment.span,
            )?,
            AssignmentKind::ArrayClone {
                destination,
                representation,
                value,
            } => self.append_instruction(
                current,
                Instruction::ArrayClone {
                    destination: *destination,
                    type_index: self
                        .layout
                        .repr_index(*representation)
                        .map_err(|error| layout_error(assignment.span, error))?,
                    value: *value,
                    span: assignment.span,
                },
                assignment.span,
            )?,
            AssignmentKind::ArraySet {
                representation,
                value,
                index,
                new_value,
                ..
            } => self.append_instruction(
                current,
                Instruction::ArraySet {
                    type_index: self
                        .layout
                        .repr_index(*representation)
                        .map_err(|error| layout_error(assignment.span, error))?,
                    value: *value,
                    index: *index,
                    new_value: *new_value,
                    span: assignment.span,
                },
                assignment.span,
            )?,
            _ => unreachable!("array dispatch received another assignment"),
        }
        Ok(current)
    }
}
