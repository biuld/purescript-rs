//! P8 conversion from Core primitives to CC-owned scalar operations.

use crate::cc::{BinaryOp, UnaryOp};
use psrs_core::{Primitive, UnaryPrimitive};

pub(super) fn lower_binary_op(value: Primitive) -> BinaryOp {
    match value {
        Primitive::IntAdd => BinaryOp::IntAdd,
        Primitive::IntSub => BinaryOp::IntSub,
        Primitive::IntMul => BinaryOp::IntMul,
        Primitive::IntQuot => BinaryOp::IntQuot,
        Primitive::IntRem => BinaryOp::IntRem,
        Primitive::IntDiv => BinaryOp::IntDiv,
        Primitive::IntMod => BinaryOp::IntMod,
        Primitive::IntAnd => BinaryOp::IntAnd,
        Primitive::IntOr => BinaryOp::IntOr,
        Primitive::IntXor => BinaryOp::IntXor,
        Primitive::IntShl => BinaryOp::IntShl,
        Primitive::IntShr => BinaryOp::IntShr,
        Primitive::IntZshr => BinaryOp::IntZshr,
        Primitive::IntEq => BinaryOp::IntEq,
        Primitive::IntNe => BinaryOp::IntNe,
        Primitive::IntLt => BinaryOp::IntLt,
        Primitive::IntLe => BinaryOp::IntLe,
        Primitive::IntGt => BinaryOp::IntGt,
        Primitive::IntGe => BinaryOp::IntGe,
        Primitive::NumberAdd => BinaryOp::NumberAdd,
        Primitive::NumberSub => BinaryOp::NumberSub,
        Primitive::NumberMul => BinaryOp::NumberMul,
        Primitive::NumberDiv => BinaryOp::NumberDiv,
        Primitive::NumberEq => BinaryOp::NumberEq,
        Primitive::NumberNe => BinaryOp::NumberNe,
        Primitive::NumberLt => BinaryOp::NumberLt,
        Primitive::NumberLe => BinaryOp::NumberLe,
        Primitive::NumberGt => BinaryOp::NumberGt,
        Primitive::NumberGe => BinaryOp::NumberGe,
        Primitive::BooleanAnd => BinaryOp::BooleanAnd,
        Primitive::BooleanOr => BinaryOp::BooleanOr,
        Primitive::BooleanEq => BinaryOp::BooleanEq,
        Primitive::BooleanNe => BinaryOp::BooleanNe,
        Primitive::CharEq => BinaryOp::CharEq,
        Primitive::CharNe => BinaryOp::CharNe,
        Primitive::CharLt => BinaryOp::CharLt,
        Primitive::CharLe => BinaryOp::CharLe,
        Primitive::CharGt => BinaryOp::CharGt,
        Primitive::CharGe => BinaryOp::CharGe,
    }
}

pub(super) fn lower_unary_op(value: UnaryPrimitive) -> UnaryOp {
    match value {
        UnaryPrimitive::IntNeg => UnaryOp::IntNeg,
        UnaryPrimitive::IntComplement => UnaryOp::IntComplement,
        UnaryPrimitive::NumberNeg => UnaryOp::NumberNeg,
        UnaryPrimitive::BooleanNot => UnaryOp::BooleanNot,
        UnaryPrimitive::IntToNumber => UnaryOp::IntToNumber,
        UnaryPrimitive::NumberToInt => UnaryOp::NumberToInt,
        UnaryPrimitive::BooleanToInt => UnaryOp::BooleanToInt,
        UnaryPrimitive::IntToBoolean => UnaryOp::IntToBoolean,
        UnaryPrimitive::CharToInt => UnaryOp::CharToInt,
        UnaryPrimitive::IntToChar => UnaryOp::IntToChar,
    }
}
