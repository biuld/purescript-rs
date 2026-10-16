//! Ordinary storage calls: CC has already adapted payloads to erased storage.
use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn lower_storage_call(
        &mut self,
        block: BlockId,
        destination: ValueId,
        import: &crate::mir::Import,
        arguments: &[ValueId],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if arguments.len() != import.parameters.len() {
            return Err(vec![BackendError::invalid_ir(
                "P9 runtime binding",
                span,
                "storage call has the wrong argument count",
            )]);
        }
        let provider = import.runtime.as_ref().ok_or_else(|| {
            vec![BackendError::invalid_ir(
                "P9 runtime binding",
                span,
                "storage call has no provider",
            )]
        })?;
        let operation = psrs_runtime::StorageOperation::from_export(&provider.function)
            .ok_or_else(|| {
                vec![BackendError::invalid_ir(
                    "P9 runtime binding",
                    span,
                    "storage call has no checked export",
                )]
            })?;
        let mut raw_arguments = Vec::with_capacity(arguments.len());
        for (value, expected) in arguments.iter().copied().zip(&import.parameters) {
            if self.value_type(value) == Some(*expected) {
                raw_arguments.push(value);
            } else if let ValueType::Ref(reference) = expected {
                let raw = self.fresh(*expected);
                self.append_instruction(
                    block,
                    Instruction::RefCast {
                        destination: raw,
                        value,
                        reference: *reference,
                        span,
                    },
                    span,
                )?;
                raw_arguments.push(raw);
            } else {
                return Err(vec![BackendError::invalid_ir(
                    "P9 runtime binding",
                    span,
                    "storage operand disagrees with its raw type",
                )]);
            }
        }
        if let Some(result) = import.result {
            let raw = if self.value_type(destination) == Some(result) {
                destination
            } else {
                self.fresh(result)
            };
            self.append_instruction(
                block,
                Instruction::Call {
                    contract: None,
                    destination: raw,
                    function: import.symbol,
                    arguments: raw_arguments,
                    span,
                },
                span,
            )?;
            if raw != destination {
                let Some(ValueType::Ref(reference)) = self.value_type(destination) else {
                    return Err(vec![BackendError::invalid_ir(
                        "P9 runtime binding",
                        span,
                        "storage result recovery requires a reference",
                    )]);
                };
                self.append_instruction(
                    block,
                    Instruction::RefCast {
                        destination,
                        value: raw,
                        reference,
                        span,
                    },
                    span,
                )?;
            }
        } else {
            self.append_instruction(
                block,
                Instruction::CallVoid {
                    function: import.symbol,
                    arguments: raw_arguments,
                    span,
                },
                span,
            )?;
            let instruction = if operation.abi().returns_normally {
                Instruction::Constant {
                    destination,
                    value: 0,
                    span,
                }
            } else {
                Instruction::Unreachable { destination, span }
            };
            self.append_instruction(block, instruction, span)?;
        }
        Ok(())
    }
}
