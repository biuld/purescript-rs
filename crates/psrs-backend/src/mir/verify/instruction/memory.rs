//! Linear-memory and integer-width instruction checks for MIR verification.

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
        Instruction::LinearAlloc {
            destination,
            bytes,
            span,
        } => {
            if *bytes == 0 {
                return Err(mir_error(
                    *span,
                    "MIR linear allocation must have nonzero size",
                ));
            }
            if value_type(function, *destination) != Some(ValueType::I32) {
                return Err(mir_error(
                    *span,
                    "MIR linear allocation must produce an i32 pointer",
                ));
            }
        }
        Instruction::LinearAllocDynamic {
            destination,
            bytes,
            span,
        } => {
            if require_value(definitions, *bytes, *span)? != ValueType::I32 {
                return Err(mir_error(
                    *span,
                    "MIR dynamic linear allocation size must be i32",
                ));
            }
            if value_type(function, *destination) != Some(ValueType::I32) {
                return Err(mir_error(
                    *span,
                    "MIR dynamic linear allocation must produce an i32 pointer",
                ));
            }
        }
        Instruction::LinearMemoryCopy {
            destination,
            source,
            bytes,
            span,
            ..
        } => {
            for value in [destination, source, bytes] {
                if require_value(definitions, *value, *span)? != ValueType::I32 {
                    return Err(mir_error(*span, "MIR memory.copy operands must be i32"));
                }
            }
        }
        Instruction::LinearLoad {
            destination,
            address,
            ty,
            span,
            ..
        } => {
            if require_value(definitions, *address, *span)? != ValueType::I32 {
                return Err(mir_error(*span, "MIR linear load address must be i32"));
            }
            if !linear_memory_type(*ty) || value_type(function, *destination) != Some(*ty) {
                return Err(mir_error(*span, "MIR linear load type is invalid"));
            }
        }
        Instruction::LinearStore {
            address,
            value,
            ty,
            span,
            ..
        } => {
            if require_value(definitions, *address, *span)? != ValueType::I32
                || require_value(definitions, *value, *span)? != *ty
                || !linear_memory_type(*ty)
            {
                return Err(mir_error(*span, "MIR linear store type is invalid"));
            }
        }
        Instruction::LinearClosureGetCapture {
            destination,
            closure,
            ty,
            span,
            ..
        } => {
            if require_value(definitions, *closure, *span)? != ValueType::I32
                || !linear_memory_type(*ty)
                || value_type(function, *destination) != Some(*ty)
            {
                return Err(mir_error(
                    *span,
                    "MIR linear closure capture type is invalid",
                ));
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

fn linear_memory_type(ty: ValueType) -> bool {
    matches!(ty, ValueType::I32 | ValueType::Boolean | ValueType::F64)
}
