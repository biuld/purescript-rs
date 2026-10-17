use super::*;
use crate::cc::{Assignment, AssignmentKind};

impl FunctionLowerer<'_> {
    pub(super) fn lower_record_assignment(
        &mut self,
        mut current: BlockId,
        assignment: &Assignment,
    ) -> Result<BlockId, Vec<BackendError>> {
        let AssignmentKind::Record {
            destination,
            record,
            operation,
        } = &assignment.kind
        else {
            unreachable!("record assignment lowering dispatch");
        };
        let storage = self
            .layout
            .representation_table()
            .record_storage()
            .ok_or_else(|| {
                vec![BackendError::invalid_ir(
                    "P9 MIR lowering",
                    assignment.span,
                    "record operation has no registered storage schema",
                )]
            })?;
        if let Some(helpers) = self.conversion_helpers.as_mut() {
            let function = helpers.intern_record(operation, storage, assignment.span);
            let mut arguments = vec![*record];
            arguments.extend(operation.arguments().iter().map(|(value, _)| *value));
            self.append_instruction(
                current,
                Instruction::Call {
                    contract: None,
                    destination: *destination,
                    function,
                    arguments,
                    span: assignment.span,
                },
                assignment.span,
            )?;
        } else {
            current = self.lower_record_operation(
                current,
                *destination,
                *record,
                operation,
                assignment.span,
            )?;
        }
        Ok(current)
    }
}
