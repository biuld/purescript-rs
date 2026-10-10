use super::*;

impl RawInvocation {
    pub(in crate::mir) fn emit(
        &self,
    ) -> Result<(Vec<ValueDecl>, Vec<Instruction>), Vec<BackendError>> {
        let mut values = Vec::new();
        let mut instructions = Vec::new();
        let mut fresh = |ty| {
            let id = ValueId(self.first_temporary + values.len() as u32);
            values.push(ValueDecl { id, ty });
            id
        };
        let mut arguments = Vec::new();
        for ((source, source_type), raw_type) in self
            .arguments
            .iter()
            .zip(&self.argument_types)
            .zip(&self.import.parameters)
        {
            let value = if source_type == raw_type {
                *source
            } else {
                let ValueType::Ref(reference) = raw_type else {
                    return Err(
                        self.error("runtime operand requires a non-reference ABI conversion")
                    );
                };
                let value = fresh(*raw_type);
                instructions.push(Instruction::RefCast {
                    destination: value,
                    value: *source,
                    reference: *reference,
                    span: self.span,
                });
                value
            };
            arguments.push(value);
        }
        match self.result {
            RawCallResult::Value => {
                let raw_type = self.import.result.unwrap();
                let result = if raw_type == self.result_type {
                    self.destination
                } else {
                    fresh(raw_type)
                };
                instructions.push(Instruction::Call {
                    destination: result,
                    function: self.import.symbol,
                    arguments,
                    span: self.span,
                });
                if result != self.destination {
                    let ValueType::Ref(reference) = self.result_type else {
                        return Err(
                            self.error("runtime result requires a non-reference ABI conversion")
                        );
                    };
                    instructions.push(Instruction::RefCast {
                        destination: self.destination,
                        value: result,
                        reference,
                        span: self.span,
                    });
                }
            }
            RawCallResult::Unit => {
                instructions.push(Instruction::CallVoid {
                    function: self.import.symbol,
                    arguments,
                    span: self.span,
                });
                instructions.push(Instruction::Constant {
                    destination: self.destination,
                    value: 0,
                    span: self.span,
                });
            }
            RawCallResult::Never => {
                instructions.push(Instruction::CallVoid {
                    function: self.import.symbol,
                    arguments,
                    span: self.span,
                });
                instructions.push(Instruction::Unreachable {
                    destination: self.destination,
                    span: self.span,
                });
            }
        }
        self.verify(&values, &instructions)?;
        Ok((values, instructions))
    }
}
