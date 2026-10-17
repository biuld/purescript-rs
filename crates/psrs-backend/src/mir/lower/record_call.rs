use super::*;
use crate::cc::{Assignment, AssignmentKind};

impl FunctionLowerer<'_> {
    pub(super) fn lower_record_primitive_assignment(
        &mut self,
        block: BlockId,
        assignment: &Assignment,
    ) -> Result<BlockId, Vec<BackendError>> {
        let AssignmentKind::RecordPrimitive {
            provider,
            record,
            operation,
        } = &assignment.kind
        else {
            unreachable!("record primitive assignment lowering dispatch");
        };
        if !self.record_primitives.contains_key(provider) {
            return Err(vec![
                BackendError::invalid_ir(
                    "P9 runtime projection",
                    assignment.span,
                    "record primitive provider was not checked before lowering",
                )
                .with_module(provider.module),
            ]);
        }
        let storage = self
            .layout
            .representation_table()
            .record_storage()
            .ok_or_else(|| {
                vec![BackendError::invalid_ir(
                    "P9 runtime projection",
                    assignment.span,
                    "record primitive has no shared storage owner",
                )]
            })?;
        if let Some(helpers) = self.conversion_helpers.as_mut() {
            let function = if matches!(operation, crate::cc::RecordOperation::Build { .. }) {
                helpers.intern_record_build(*provider, operation, storage, assignment.span)
            } else {
                helpers.intern_record(operation, storage, assignment.span)
            };
            let mut arguments = Vec::new();
            if let Some(record) = record {
                arguments.push(*record);
            }
            arguments.extend(operation.arguments(storage).iter().map(|(value, _)| *value));
            self.append_instruction(
                block,
                Instruction::Call {
                    contract: None,
                    destination: assignment.destination,
                    function,
                    arguments,
                    span: assignment.span,
                },
                assignment.span,
            )?;
            Ok(block)
        } else if let crate::cc::RecordOperation::Build { labels, values } = operation {
            self.lower_record_build_operation(
                block,
                assignment.destination,
                *labels,
                *values,
                assignment.span,
            )
        } else {
            let record = record.expect("non-build record operation has a record operand");
            self.lower_record_operation(
                block,
                assignment.destination,
                record,
                operation,
                assignment.span,
            )
        }
    }

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
            arguments.extend(operation.arguments(storage).iter().map(|(value, _)| *value));
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
