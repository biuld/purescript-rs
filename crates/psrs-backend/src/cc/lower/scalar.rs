//! P8 conversion from Core intrinsic identities to CC-owned scalar operations.

use crate::cc::{BinaryOp, UnaryOp};
use psrs_hir::Intrinsic;

pub(super) fn lower_binary_op(value: Intrinsic) -> BinaryOp {
    match value {
        Intrinsic::I32Add => BinaryOp::IntAdd,
        Intrinsic::I32Sub => BinaryOp::IntSub,
        Intrinsic::I32Mul => BinaryOp::IntMul,
        Intrinsic::I32DivS => BinaryOp::IntQuot,
        Intrinsic::I32RemS => BinaryOp::IntRem,
        Intrinsic::IntDiv => BinaryOp::IntDiv,
        Intrinsic::IntMod => BinaryOp::IntMod,
        Intrinsic::IntAnd => BinaryOp::IntAnd,
        Intrinsic::IntOr => BinaryOp::IntOr,
        Intrinsic::IntXor => BinaryOp::IntXor,
        Intrinsic::IntShl => BinaryOp::IntShl,
        Intrinsic::IntShr => BinaryOp::IntShr,
        Intrinsic::IntZshr => BinaryOp::IntZshr,
        Intrinsic::I32Eq => BinaryOp::IntEq,
        Intrinsic::I32Ne => BinaryOp::IntNe,
        Intrinsic::I32LtS => BinaryOp::IntLt,
        Intrinsic::I32LeS => BinaryOp::IntLe,
        Intrinsic::I32GtS => BinaryOp::IntGt,
        Intrinsic::I32GeS => BinaryOp::IntGe,
        Intrinsic::NumberAdd => BinaryOp::NumberAdd,
        Intrinsic::NumberSub => BinaryOp::NumberSub,
        Intrinsic::NumberMul => BinaryOp::NumberMul,
        Intrinsic::NumberDiv => BinaryOp::NumberDiv,
        Intrinsic::NumberEq => BinaryOp::NumberEq,
        Intrinsic::NumberNe => BinaryOp::NumberNe,
        Intrinsic::NumberLt => BinaryOp::NumberLt,
        Intrinsic::NumberLe => BinaryOp::NumberLe,
        Intrinsic::NumberGt => BinaryOp::NumberGt,
        Intrinsic::NumberGe => BinaryOp::NumberGe,
        Intrinsic::BooleanAnd => BinaryOp::BooleanAnd,
        Intrinsic::BooleanOr => BinaryOp::BooleanOr,
        Intrinsic::BooleanEq => BinaryOp::BooleanEq,
        Intrinsic::BooleanNe => BinaryOp::BooleanNe,
        Intrinsic::CharEq => BinaryOp::CharEq,
        Intrinsic::CharNe => BinaryOp::CharNe,
        Intrinsic::CharLt => BinaryOp::CharLt,
        Intrinsic::CharLe => BinaryOp::CharLe,
        Intrinsic::CharGt => BinaryOp::CharGt,
        Intrinsic::CharGe => BinaryOp::CharGe,
        _ => unreachable!(
            "lower_binary_op is only called for a binary scalar intrinsic, got {value:?}"
        ),
    }
}

pub(super) fn lower_unary_op(value: Intrinsic) -> UnaryOp {
    match value {
        Intrinsic::IntNeg => UnaryOp::IntNeg,
        Intrinsic::IntComplement => UnaryOp::IntComplement,
        Intrinsic::NumberNeg => UnaryOp::NumberNeg,
        Intrinsic::NumberTrunc => UnaryOp::NumberTrunc,
        Intrinsic::BooleanNot => UnaryOp::BooleanNot,
        Intrinsic::IntToNumber => UnaryOp::IntToNumber,
        Intrinsic::NumberToInt => UnaryOp::NumberToInt,
        Intrinsic::BooleanToInt => UnaryOp::BooleanToInt,
        Intrinsic::IntToBoolean => UnaryOp::IntToBoolean,
        Intrinsic::CharToInt => UnaryOp::CharToInt,
        Intrinsic::IntToChar => UnaryOp::IntToChar,
        _ => unreachable!(
            "lower_unary_op is only called for a unary scalar intrinsic, got {value:?}"
        ),
    }
}
