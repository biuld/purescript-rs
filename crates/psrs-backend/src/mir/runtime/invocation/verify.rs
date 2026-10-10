use super::*;

impl RawInvocation {
    /// Check operand and result witnesses against the source call. Mere type
    /// agreement, or the presence of some call, is not sufficient evidence.
    pub(in crate::mir) fn verify(
        &self,
        values: &[ValueDecl],
        instructions: &[Instruction],
    ) -> Result<(), Vec<BackendError>> {
        let mut cursor = 0;
        let mut temporary = 0;
        let mut expect_fresh = |id, ty| {
            let expected = values.get(temporary);
            let valid = expected.is_some_and(|value| value.id == id && value.ty == ty)
                && id == ValueId(self.first_temporary + temporary as u32);
            temporary += 1;
            valid
        };
        let mut arguments = Vec::new();
        for ((source, source_type), raw_type) in self
            .arguments
            .iter()
            .zip(&self.argument_types)
            .zip(&self.import.parameters)
        {
            if source_type == raw_type {
                arguments.push(*source);
                continue;
            }
            let Some(Instruction::RefCast {
                destination,
                value,
                reference,
                ..
            }) = instructions.get(cursor)
            else {
                return Err(self.error("runtime operand has no checked cast witness"));
            };
            if value != source
                || ValueType::Ref(*reference) != *raw_type
                || !expect_fresh(*destination, *raw_type)
            {
                return Err(
                    self.error("runtime operand cast changes source identity or target type")
                );
            }
            arguments.push(*destination);
            cursor += 1;
        }
        match self.result {
            RawCallResult::Value => {
                let Some(Instruction::Call {
                    destination,
                    function,
                    arguments: actual,
                    ..
                }) = instructions.get(cursor)
                else {
                    return Err(self.error("runtime value result has no actual call"));
                };
                if *function != self.import.symbol || *actual != arguments {
                    return Err(
                        self.error("runtime call changes producer or operand correspondence")
                    );
                }
                cursor += 1;
                let raw_type = self.import.result.unwrap();
                if raw_type == self.result_type {
                    if *destination != self.destination {
                        return Err(self.error("runtime call changes its source destination"));
                    }
                } else {
                    if !expect_fresh(*destination, raw_type) {
                        return Err(self.error("runtime raw result has no typed temporary"));
                    }
                    let Some(Instruction::RefCast {
                        destination: output,
                        value,
                        reference,
                        ..
                    }) = instructions.get(cursor)
                    else {
                        return Err(self.error("runtime result has no checked recovery witness"));
                    };
                    if *output != self.destination
                        || value != destination
                        || ValueType::Ref(*reference) != self.result_type
                    {
                        return Err(self.error(
                            "runtime recovery loses the actual call result or checked payload type",
                        ));
                    }
                    cursor += 1;
                }
            }
            RawCallResult::Unit => {
                if !matches!(instructions.get(cursor), Some(Instruction::CallVoid { function, arguments: actual, .. })
                    if *function == self.import.symbol && *actual == arguments)
                {
                    return Err(self.error("runtime Unit requires an actual matching void call"));
                }
                cursor += 1;
                if self.result_type != ValueType::I32
                    || !matches!(instructions.get(cursor),
                    Some(Instruction::Constant { destination, value: 0, .. }) if *destination == self.destination)
                {
                    return Err(
                        self.error("runtime Unit result must follow its actual void invocation")
                    );
                }
                cursor += 1;
            }
            RawCallResult::Never => {
                if self.import.result.is_some()
                    || !matches!(instructions.get(cursor), Some(Instruction::CallVoid { function, arguments: actual, .. })
                        if *function == self.import.symbol && *actual == arguments)
                    || !matches!(instructions.get(cursor + 1), Some(Instruction::Unreachable { destination, .. })
                        if *destination == self.destination)
                {
                    return Err(self.error(
                        "non-returning invocation requires its actual call and bottom projection",
                    ));
                }
                cursor += 2;
            }
        }
        if cursor != instructions.len() || temporary != values.len() {
            return Err(
                self.error("runtime invocation adds unaccounted instructions or temporaries")
            );
        }
        Ok(())
    }
}
