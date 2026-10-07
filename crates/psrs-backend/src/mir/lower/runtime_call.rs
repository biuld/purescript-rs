//! Language-value transport selected by the artifact's checked value protocol.
use super::*;
use crate::target_intrinsics::{Implementation, implementation};
use psrs_runtime::RawCallProtocol;

impl FunctionLowerer<'_> {
    pub(super) fn lower_runtime_call(
        &mut self,
        block: BlockId,
        destination: ValueId,
        intrinsic: psrs_hir::Intrinsic,
        arguments: &[ValueId],
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        let Implementation::Artifact(provider) = implementation(intrinsic) else {
            return Err(vec![BackendError::invalid_ir(
                "P9 MIR lowering",
                span,
                "runtime call has no artifact provider",
            )]);
        };
        provider
            .validate_protocol()
            .map_err(|error| vec![BackendError::invalid_ir("P9 MIR lowering", span, error)])?;
        match provider.abi.protocol {
            RawCallProtocol::Scalars => {
                self.emit_artifact_call(block, destination, provider, arguments.to_vec(), span)
            }
            RawCallProtocol::Utf8Input => {
                let mut raw = Vec::new();
                let mut frees = Vec::new();
                crate::mir::wit::lower_string(
                    self,
                    arguments[0],
                    &mut raw,
                    &mut frees,
                    block,
                    span,
                )?;
                self.emit_artifact_call(block, destination, provider, raw, span)?;
                for pending in frees {
                    crate::mir::wit::free_buffer(
                        self,
                        pending.pointer,
                        pending.length,
                        pending.align,
                        block,
                        span,
                    )?;
                }
                Ok(())
            }
            RawCallProtocol::Utf8Output { capacity } => {
                let zero = self.constant(block, 0, span)?;
                let align = self.constant(block, 1, span)?;
                let capacity = self.constant(block, capacity as i32, span)?;
                let buffer = self.fresh(ValueType::I32);
                self.append_instruction(
                    block,
                    Instruction::Call {
                        destination: buffer,
                        function: crate::abi::REALLOC_SYMBOL,
                        arguments: vec![zero, zero, align, capacity],
                        span,
                    },
                    span,
                )?;
                let length = self.fresh(ValueType::I32);
                let mut raw = arguments.to_vec();
                raw.extend([buffer, capacity]);
                self.emit_artifact_call(block, length, provider, raw, span)?;
                self.append_instruction(
                    block,
                    Instruction::Call {
                        destination,
                        function: crate::abi::BYTES_TO_STRING_SYMBOL,
                        arguments: vec![buffer, length],
                        span,
                    },
                    span,
                )?;
                crate::mir::wit::free_buffer(self, buffer, capacity, 1, block, span)
            }
        }
    }

    fn emit_artifact_call(
        &mut self,
        block: BlockId,
        destination: ValueId,
        provider: &crate::target_runtime::ArtifactImplementation,
        arguments: Vec<ValueId>,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if provider.abi.result.is_some() {
            self.append_instruction(
                block,
                Instruction::Call {
                    destination,
                    function: provider.symbol,
                    arguments,
                    span,
                },
                span,
            )
        } else {
            self.append_instruction(
                block,
                Instruction::CallVoid {
                    function: provider.symbol,
                    arguments,
                    span,
                },
                span,
            )?;
            self.append_instruction(
                block,
                Instruction::Constant {
                    destination,
                    value: 0,
                    span,
                },
                span,
            )
        }
    }
}
