//! Linear-memory and integer-width instruction checks for MIR verification.

use super::super::util::{mir_error, require_value, value_type};
use crate::BackendError;
use crate::mir::{Function, Instruction, ValueId, ValueType};
use std::collections::HashMap;

pub(super) fn verify_memory_instruction(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
) -> Result<(), Vec<BackendError>> {
    match instruction {
        Instruction::Load {
            destination,
            address,
            span,
            ..
        } => {
            if require_value(definitions, *address, *span)? != ValueType::I32 {
                return Err(mir_error(*span, "MIR load address must be i32"));
            }
            if value_type(function, *destination) != Some(ValueType::I32) {
                return Err(mir_error(*span, "MIR load result must be i32"));
            }
        }
        Instruction::Store {
            address,
            value,
            span,
            ..
        } => {
            if require_value(definitions, *address, *span)? != ValueType::I32 {
                return Err(mir_error(*span, "MIR store address must be i32"));
            }
            if require_value(definitions, *value, *span)? != ValueType::I32 {
                return Err(mir_error(*span, "MIR store value must be i32"));
            }
        }
        Instruction::WrapI64 {
            destination,
            value,
            span,
        } => {
            if require_value(definitions, *value, *span)? != ValueType::I64
                || value_type(function, *destination) != Some(ValueType::I32)
            {
                return Err(mir_error(
                    *span,
                    "MIR i32.wrap_i64 operand or result type is invalid",
                ));
            }
        }
        Instruction::WidenI64 {
            destination,
            value,
            span,
            ..
        } => {
            if require_value(definitions, *value, *span)? != ValueType::I32
                || value_type(function, *destination) != Some(ValueType::I64)
            {
                return Err(mir_error(
                    *span,
                    "MIR i64.extend_i32 operand or result type is invalid",
                ));
            }
        }
        _ => unreachable!("memory verifier received a non-memory instruction"),
    }
    Ok(())
}
