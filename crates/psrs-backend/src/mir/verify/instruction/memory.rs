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
    allocation_bounds: &HashMap<ValueId, u32>,
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
            alignment,
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
            verify_allocation_alignment(*alignment, Some(*bytes), *span)?;
        }
        Instruction::LinearAllocDynamic {
            destination,
            bytes,
            alignment,
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
            verify_allocation_alignment(*alignment, None, *span)?;
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
            memory,
            offset,
            object_bytes,
            alignment,
            ty,
            span,
            ..
        } => {
            verify_linear_access(*memory, *alignment, *ty, *span)?;
            verify_access_bounds(
                *address,
                *offset,
                *object_bytes,
                *ty,
                allocation_bounds,
                *span,
            )?;
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
            memory,
            offset,
            object_bytes,
            alignment,
            ty,
            span,
            ..
        } => {
            verify_linear_access(*memory, *alignment, *ty, *span)?;
            verify_access_bounds(
                *address,
                *offset,
                *object_bytes,
                *ty,
                allocation_bounds,
                *span,
            )?;
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
            offset,
            object_bytes,
            ty,
            span,
            ..
        } => {
            verify_access_bounds(
                *closure,
                *offset,
                *object_bytes,
                *ty,
                allocation_bounds,
                *span,
            )?;
            if require_value(definitions, *closure, *span)? != ValueType::I32
                || !linear_memory_type(*ty)
                || *offset < 8
                || !offset.is_multiple_of(value_alignment(*ty))
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

fn value_alignment(ty: ValueType) -> u32 {
    match ty {
        ValueType::F64 => 8,
        ValueType::I32 | ValueType::Boolean => 4,
        _ => 1,
    }
}

fn verify_access_bounds(
    address: ValueId,
    offset: u32,
    object_bytes: u32,
    ty: ValueType,
    allocation_bounds: &HashMap<ValueId, u32>,
    span: psrs_span::TextRange,
) -> Result<(), Vec<BackendError>> {
    let access_bytes = match ty {
        ValueType::I32 | ValueType::Boolean => 4,
        ValueType::F64 => 8,
        _ => return Err(mir_error(span, "MIR linear access type is invalid")),
    };
    if offset
        .checked_add(access_bytes)
        .is_none_or(|end| end > object_bytes)
    {
        return Err(mir_error(
            span,
            "MIR linear access exceeds its planned object bounds",
        ));
    }
    if allocation_bounds
        .get(&address)
        .is_some_and(|allocation_bytes| object_bytes > *allocation_bytes)
    {
        return Err(mir_error(
            span,
            "MIR linear access bound exceeds its originating allocation",
        ));
    }
    Ok(())
}

fn verify_allocation_alignment(
    alignment: u32,
    bytes: Option<u32>,
    span: psrs_span::TextRange,
) -> Result<(), Vec<BackendError>> {
    if !matches!(alignment, 4 | 8) || bytes.is_some_and(|bytes| !bytes.is_multiple_of(alignment)) {
        return Err(mir_error(
            span,
            "MIR linear allocation alignment is invalid",
        ));
    }
    Ok(())
}

fn verify_linear_access(
    memory: MemoryId,
    alignment: u32,
    ty: ValueType,
    span: psrs_span::TextRange,
) -> Result<(), Vec<BackendError>> {
    if memory != MemoryId(0) {
        return Err(mir_error(
            span,
            "MIR linear access references an unknown memory",
        ));
    }
    let natural = match ty {
        ValueType::I32 | ValueType::Boolean => 4,
        ValueType::F64 => 8,
        _ => return Err(mir_error(span, "MIR linear access type is invalid")),
    };
    if !alignment.is_power_of_two() || alignment > natural {
        return Err(mir_error(span, "MIR linear access alignment is invalid"));
    }
    Ok(())
}
