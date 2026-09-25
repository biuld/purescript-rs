use super::super::util::{mir_error, require_value, value_type};
use crate::BackendError;
use crate::mir::NumericOp;
use crate::mir::{Function, Instruction, ValueId, ValueType};
use std::collections::HashMap;

pub(super) fn verify_primitive(
    function: &Function,
    instruction: &Instruction,
    definitions: &HashMap<ValueId, ValueType>,
) -> Result<(), Vec<BackendError>> {
    let Instruction::Primitive {
        destination,
        op,
        left,
        right,
        span,
    } = instruction
    else {
        unreachable!("primitive verifier received another instruction")
    };
    let left_ty = require_value(definitions, *left, *span)?;
    let right_ty = require_value(definitions, *right, *span)?;
    let result_ty = value_type(function, *destination)
        .ok_or_else(|| mir_error(*span, "missing MIR result type"))?;
    let (expected_operand, expected_result) = match op {
        NumericOp::I32Eq
        | NumericOp::I32Ne
        | NumericOp::I32LtS
        | NumericOp::I32LeS
        | NumericOp::I32GtS
        | NumericOp::I32GeS => (ValueType::I32, ValueType::Boolean),
        NumericOp::F64Eq
        | NumericOp::F64Ne
        | NumericOp::F64Lt
        | NumericOp::F64Le
        | NumericOp::F64Gt
        | NumericOp::F64Ge => (ValueType::F64, ValueType::Boolean),
        NumericOp::F64Add | NumericOp::F64Sub | NumericOp::F64Mul | NumericOp::F64Div => {
            (ValueType::F64, ValueType::F64)
        }
        NumericOp::BoolAnd | NumericOp::BoolOr | NumericOp::BoolEq | NumericOp::BoolNe => {
            (ValueType::Boolean, ValueType::Boolean)
        }
        _ => (ValueType::I32, ValueType::I32),
    };
    if left_ty != expected_operand || right_ty != expected_operand || result_ty != expected_result {
        return Err(mir_error(
            *span,
            "MIR primitive operand or result type is invalid",
        ));
    }
    Ok(())
}
