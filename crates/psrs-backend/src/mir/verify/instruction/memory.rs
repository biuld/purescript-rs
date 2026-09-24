//! Canonical ABI memory and integer-width instruction checks for MIR
//! verification. Linear memory is the byte-oriented ABI boundary, not a
//! language heap (`DEC-09`).

use super::super::util::{mir_error, require_value, value_type};
use crate::BackendError;
use crate::mir::{Function, Instruction, ValueId, ValueType};
use crate::types::MemoryId;
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
            memory,
            span,
            ..
        }
        | Instruction::Load8U {
            destination,
            address,
            memory,
            span,
            ..
        } => {
            if *memory != MemoryId(0) {
                return Err(mir_error(*span, "MIR load references an unknown memory"));
            }
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
            memory,
            span,
            ..
        } => {
            if *memory != MemoryId(0) {
                return Err(mir_error(*span, "MIR store references an unknown memory"));
            }
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
