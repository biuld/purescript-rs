use super::helpers::{require_destination, require_value_shape};
use crate::BackendError;
use crate::cc::{Assignment, BinaryOp, UnaryOp, ValueShape};
use crate::types::ValueId;
use std::collections::HashMap;

pub(super) fn verify_binary_operation(
    op: BinaryOp,
    left: ValueId,
    right: ValueId,
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
) -> Result<(), Vec<BackendError>> {
    use BinaryOp::*;

    let (operand, result) = match op {
        IntAdd | IntSub | IntMul | IntQuot | IntRem | IntDiv | IntMod | IntAnd | IntOr | IntXor
        | IntShl | IntShr | IntZshr => (ValueShape::Integer, ValueShape::Integer),
        IntEq | IntNe | IntLt | IntLe | IntGt | IntGe | CharEq | CharNe | CharLt | CharLe
        | CharGt | CharGe => (ValueShape::Integer, ValueShape::Boolean),
        NumberAdd | NumberSub | NumberMul | NumberDiv => (ValueShape::Number, ValueShape::Number),
        NumberEq | NumberNe | NumberLt | NumberLe | NumberGt | NumberGe => {
            (ValueShape::Number, ValueShape::Boolean)
        }
        BooleanAnd | BooleanOr | BooleanEq | BooleanNe => {
            (ValueShape::Boolean, ValueShape::Boolean)
        }
    };
    require_value_shape(declared, left, operand, assignment)?;
    require_value_shape(declared, right, operand, assignment)?;
    require_destination(
        declared,
        assignment,
        result,
        "primitive operation has an incompatible result shape",
    )
}

pub(super) fn verify_unary_operation(
    op: UnaryOp,
    value: ValueId,
    assignment: &Assignment,
    declared: &HashMap<ValueId, ValueShape>,
) -> Result<(), Vec<BackendError>> {
    use UnaryOp::*;

    let (operand, result) = match op {
        IntNeg | IntComplement | CharToInt | IntToChar => {
            (ValueShape::Integer, ValueShape::Integer)
        }
        NumberNeg => (ValueShape::Number, ValueShape::Number),
        BooleanNot => (ValueShape::Boolean, ValueShape::Boolean),
        IntToNumber => (ValueShape::Integer, ValueShape::Number),
        NumberToInt => (ValueShape::Number, ValueShape::Integer),
        BooleanToInt => (ValueShape::Boolean, ValueShape::Integer),
        IntToBoolean => (ValueShape::Integer, ValueShape::Boolean),
    };
    require_value_shape(declared, value, operand, assignment)?;
    require_destination(
        declared,
        assignment,
        result,
        "unary operation has an incompatible result shape",
    )
}
