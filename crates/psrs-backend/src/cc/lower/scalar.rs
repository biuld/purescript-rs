//! P8 conversion from Core primitives to CC-owned scalar operations.

use crate::cc::BinaryOp;
use psrs_core::Primitive;

pub(super) fn lower_binary_op(value: Primitive) -> BinaryOp {
    match value {
        Primitive::Add => BinaryOp::IntAdd,
        Primitive::Sub => BinaryOp::IntSub,
        Primitive::Mul => BinaryOp::IntMul,
        Primitive::DivS => BinaryOp::IntQuot,
        Primitive::RemS => BinaryOp::IntRem,
        Primitive::Eq => BinaryOp::IntEq,
        Primitive::Ne => BinaryOp::IntNe,
        Primitive::LtS => BinaryOp::IntLt,
        Primitive::LeS => BinaryOp::IntLe,
        Primitive::GtS => BinaryOp::IntGt,
        Primitive::GeS => BinaryOp::IntGe,
    }
}
