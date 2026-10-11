use super::{Instruction, Module};
use crate::BackendError;

impl Module {
    /// Source contracts remain immutable and inspectable after physical layout.
    pub fn control_contracts(&self) -> &[crate::cc::ControlContract] {
        self.layout.as_ref().map_or(&[], |layout| {
            layout.representation_table().control_contracts.as_slice()
        })
    }
}

pub(super) fn verify(module: &Module, errors: &mut Vec<BackendError>) {
    for function in &module.functions {
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            let Instruction::ControlEvidence {
                arguments,
                contract,
                span,
                ..
            } = instruction
            else {
                continue;
            };
            let mut message = match module.control_contracts().get(contract.0 as usize) {
                None => Some("MIR control evidence has no checked source contract"),
                Some(proof) if arguments.len() != proof.arguments().len() => Some(
                    "MIR control evidence argument count disagrees with its checked source contract",
                ),
                _ => None,
            };
            if message.is_none() {
                let proof = &module.control_contracts()[contract.0 as usize];
                for (argument, expected) in arguments.iter().zip(proof.argument_identities()) {
                    let mut value = *argument;
                    let definitions = function
                        .blocks
                        .iter()
                        .flat_map(|block| &block.instructions)
                        .collect::<Vec<_>>();
                    for _ in 0..definitions.len() {
                        match definitions
                            .iter()
                            .find(|instruction| instruction.destination() == Some(value))
                        {
                            Some(
                                Instruction::Copy { value: source, .. }
                                | Instruction::RefCast { value: source, .. },
                            ) => value = *source,
                            Some(Instruction::ControlEvidence { contract, .. }) => {
                                if module
                                    .control_contracts()
                                    .get(contract.0 as usize)
                                    .is_none_or(|producer| producer.result_identity() != *expected)
                                {
                                    message = Some(
                                        "MIR control operand source type disagrees with its checked domain",
                                    );
                                }
                                break;
                            }
                            _ => break,
                        }
                    }
                }
            }
            if let Some(message) = message {
                errors.push(
                    BackendError::invalid_ir("P9 MIR verification", *span, message)
                        .with_module(function.symbol.module),
                );
            }
        }
    }
}
