use super::super::util::{mir_error, require_value, value_type};
use crate::BackendError;
use crate::mir::{Function, Instruction, ValueId, ValueType};
use std::collections::HashMap;

pub(super) fn verify_copy(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
) -> Result<(), Vec<BackendError>> {
    let Instruction::Copy {
        destination,
        value,
        span,
    } = instruction
    else {
        unreachable!("copy verifier received another instruction")
    };
    if value_type(function, *destination) != Some(require_value(definitions, *value, *span)?) {
        return Err(mir_error(*span, "MIR copy operand and result types differ"));
    }
    Ok(())
}
