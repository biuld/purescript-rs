//! Concrete scalar operations selected by P9 for the Wasm target.

use crate::cc::{BinaryOp, UnaryOp as CcUnaryOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    I32Neg,
    I32Complement,
    F64Neg,
    BoolNot,
    I32ToF64,
    F64ToF32,
    F32ToF64,
    F64ToI32Sat,
    BoolToI32,
    I32ToBool,
    I32Identity,
}

impl From<CcUnaryOp> for UnaryOp {
    fn from(value: CcUnaryOp) -> Self {
        match value {
            CcUnaryOp::IntNeg => Self::I32Neg,
            CcUnaryOp::IntComplement => Self::I32Complement,
            CcUnaryOp::NumberNeg => Self::F64Neg,
            CcUnaryOp::BooleanNot => Self::BoolNot,
            CcUnaryOp::IntToNumber => Self::I32ToF64,
            CcUnaryOp::NumberToInt => Self::F64ToI32Sat,
            CcUnaryOp::BooleanToInt => Self::BoolToI32,
            CcUnaryOp::IntToBoolean => Self::I32ToBool,
            CcUnaryOp::CharToInt | CcUnaryOp::IntToChar => Self::I32Identity,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericOp {
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32RemS,
    I32And,
    I32Or,
    I32Xor,
    I32Shl,
    I32ShrS,
    I32ShrU,
    I32Eq,
    I32Ne,
    I32LtS,
    I32LeS,
    I32GtS,
    I32GeS,
    BoolAnd,
    BoolOr,
    BoolEq,
    BoolNe,
    F64Add,
    F64Sub,
    F64Mul,
    F64Div,
    F64Eq,
    F64Ne,
    F64Lt,
    F64Le,
    F64Gt,
    F64Ge,
}

impl TryFrom<BinaryOp> for NumericOp {
    type Error = BinaryOp;

    fn try_from(value: BinaryOp) -> Result<Self, Self::Error> {
        Ok(match value {
            BinaryOp::IntAdd => Self::I32Add,
            BinaryOp::IntSub => Self::I32Sub,
            BinaryOp::IntMul => Self::I32Mul,
            BinaryOp::IntQuot => Self::I32DivS,
            BinaryOp::IntRem => Self::I32RemS,
            BinaryOp::IntDiv | BinaryOp::IntMod => return Err(value),
            BinaryOp::IntAnd => Self::I32And,
            BinaryOp::IntOr => Self::I32Or,
            BinaryOp::IntXor => Self::I32Xor,
            BinaryOp::IntShl => Self::I32Shl,
            BinaryOp::IntShr => Self::I32ShrS,
            BinaryOp::IntZshr => Self::I32ShrU,
            BinaryOp::IntEq => Self::I32Eq,
            BinaryOp::IntNe => Self::I32Ne,
            BinaryOp::IntLt => Self::I32LtS,
            BinaryOp::IntLe => Self::I32LeS,
            BinaryOp::IntGt => Self::I32GtS,
            BinaryOp::IntGe => Self::I32GeS,
            BinaryOp::NumberAdd => Self::F64Add,
            BinaryOp::NumberSub => Self::F64Sub,
            BinaryOp::NumberMul => Self::F64Mul,
            BinaryOp::NumberDiv => Self::F64Div,
            BinaryOp::NumberEq => Self::F64Eq,
            BinaryOp::NumberNe => Self::F64Ne,
            BinaryOp::NumberLt => Self::F64Lt,
            BinaryOp::NumberLe => Self::F64Le,
            BinaryOp::NumberGt => Self::F64Gt,
            BinaryOp::NumberGe => Self::F64Ge,
            BinaryOp::BooleanAnd => Self::BoolAnd,
            BinaryOp::BooleanOr => Self::BoolOr,
            BinaryOp::BooleanEq => Self::BoolEq,
            BinaryOp::BooleanNe => Self::BoolNe,
            BinaryOp::CharEq => Self::I32Eq,
            BinaryOp::CharNe => Self::I32Ne,
            BinaryOp::CharLt => Self::I32LtS,
            BinaryOp::CharLe => Self::I32LeS,
            BinaryOp::CharGt => Self::I32GtS,
            BinaryOp::CharGe => Self::I32GeS,
        })
    }
}
