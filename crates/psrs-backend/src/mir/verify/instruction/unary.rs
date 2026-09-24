use super::super::util::{mir_error, require_value, value_type};
use crate::BackendError;
use crate::mir::{Function, Instruction, UnaryOp, ValueType};
use std::collections::HashMap;

pub(super) fn verify_unary(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<crate::types::ValueId, ValueType>,
) -> Result<(), Vec<BackendError>> {
    let Instruction::UnaryPrimitive {
        destination,
        op,
        value,
        span,
    } = instruction
    else {
        unreachable!("unary verifier received another instruction")
    };
    let operand_type = require_value(definitions, *value, *span)?;
    let result_type = value_type(function, *destination)
        .ok_or_else(|| mir_error(*span, "missing MIR unary result type"))?;
    let (expected_operand, expected_result) = match op {
        UnaryOp::I32Neg | UnaryOp::I32Complement | UnaryOp::I32Identity => {
            (ValueType::I32, ValueType::I32)
        }
        UnaryOp::F64Neg => (ValueType::F64, ValueType::F64),
        UnaryOp::BoolNot => (ValueType::Boolean, ValueType::Boolean),
        UnaryOp::I32ToF64 => (ValueType::I32, ValueType::F64),
        UnaryOp::F64ToF32 => (ValueType::F64, ValueType::F32),
        UnaryOp::F32ToF64 => (ValueType::F32, ValueType::F64),
        UnaryOp::F64ToI32Sat => (ValueType::F64, ValueType::I32),
        UnaryOp::BoolToI32 => (ValueType::Boolean, ValueType::I32),
        UnaryOp::I32ToBool => (ValueType::I32, ValueType::Boolean),
    };
    if operand_type != expected_operand || result_type != expected_result {
        return Err(mir_error(
            *span,
            "MIR unary operand or result type is invalid",
        ));
    }
    Ok(())
}
