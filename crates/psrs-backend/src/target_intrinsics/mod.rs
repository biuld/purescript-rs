//! Exhaustive target implementation selection for checked language identities.
pub(crate) mod generated;
use crate::cc::{BinaryOp, UnaryOp};
use crate::target_runtime::{self, ArtifactImplementation};
use psrs_hir::Intrinsic;

#[derive(Clone, Copy, Debug)]
pub(crate) enum ScalarOperation {
    Unary(UnaryOp),
    Binary(BinaryOp),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GeneratedOperation {
    ArrayLength,
    ArrayIndex,
    ArrayUpdate,
    ArrayAppend,
    ArrayFill,
    ArrayWrite,
    StringToBytes,
    BytesToString,
    UnsafeCoerce,
}

pub(crate) enum Implementation {
    Direct(ScalarOperation),
    Generated(GeneratedOperation),
    Artifact(&'static ArtifactImplementation),
    Elaborated,
    Unsupported,
}

pub(crate) fn implementation(intrinsic: Intrinsic) -> Implementation {
    use Implementation::*;
    match intrinsic {
        Intrinsic::IntAdd => Direct(ScalarOperation::Binary(BinaryOp::IntAdd)),
        Intrinsic::IntSub => Direct(ScalarOperation::Binary(BinaryOp::IntSub)),
        Intrinsic::IntMul => Direct(ScalarOperation::Binary(BinaryOp::IntMul)),
        Intrinsic::IntQuot => Direct(ScalarOperation::Binary(BinaryOp::IntQuot)),
        Intrinsic::IntRem => Direct(ScalarOperation::Binary(BinaryOp::IntRem)),
        Intrinsic::IntAnd => Direct(ScalarOperation::Binary(BinaryOp::IntAnd)),
        Intrinsic::IntOr => Direct(ScalarOperation::Binary(BinaryOp::IntOr)),
        Intrinsic::IntXor => Direct(ScalarOperation::Binary(BinaryOp::IntXor)),
        Intrinsic::IntShl => Direct(ScalarOperation::Binary(BinaryOp::IntShl)),
        Intrinsic::IntShr => Direct(ScalarOperation::Binary(BinaryOp::IntShr)),
        Intrinsic::IntZshr => Direct(ScalarOperation::Binary(BinaryOp::IntZshr)),
        Intrinsic::IntEq => Direct(ScalarOperation::Binary(BinaryOp::IntEq)),
        Intrinsic::IntNe => Direct(ScalarOperation::Binary(BinaryOp::IntNe)),
        Intrinsic::IntLt => Direct(ScalarOperation::Binary(BinaryOp::IntLt)),
        Intrinsic::IntLe => Direct(ScalarOperation::Binary(BinaryOp::IntLe)),
        Intrinsic::IntGt => Direct(ScalarOperation::Binary(BinaryOp::IntGt)),
        Intrinsic::IntGe => Direct(ScalarOperation::Binary(BinaryOp::IntGe)),
        Intrinsic::NumberAdd => Direct(ScalarOperation::Binary(BinaryOp::NumberAdd)),
        Intrinsic::NumberSub => Direct(ScalarOperation::Binary(BinaryOp::NumberSub)),
        Intrinsic::NumberMul => Direct(ScalarOperation::Binary(BinaryOp::NumberMul)),
        Intrinsic::NumberDiv => Direct(ScalarOperation::Binary(BinaryOp::NumberDiv)),
        Intrinsic::NumberEq => Direct(ScalarOperation::Binary(BinaryOp::NumberEq)),
        Intrinsic::NumberNe => Direct(ScalarOperation::Binary(BinaryOp::NumberNe)),
        Intrinsic::NumberLt => Direct(ScalarOperation::Binary(BinaryOp::NumberLt)),
        Intrinsic::NumberLe => Direct(ScalarOperation::Binary(BinaryOp::NumberLe)),
        Intrinsic::NumberGt => Direct(ScalarOperation::Binary(BinaryOp::NumberGt)),
        Intrinsic::NumberGe => Direct(ScalarOperation::Binary(BinaryOp::NumberGe)),
        Intrinsic::BooleanAnd => Direct(ScalarOperation::Binary(BinaryOp::BooleanAnd)),
        Intrinsic::BooleanOr => Direct(ScalarOperation::Binary(BinaryOp::BooleanOr)),
        Intrinsic::BooleanEq => Direct(ScalarOperation::Binary(BinaryOp::BooleanEq)),
        Intrinsic::BooleanNe => Direct(ScalarOperation::Binary(BinaryOp::BooleanNe)),
        Intrinsic::CharEq => Direct(ScalarOperation::Binary(BinaryOp::CharEq)),
        Intrinsic::CharNe => Direct(ScalarOperation::Binary(BinaryOp::CharNe)),
        Intrinsic::CharLt => Direct(ScalarOperation::Binary(BinaryOp::CharLt)),
        Intrinsic::CharLe => Direct(ScalarOperation::Binary(BinaryOp::CharLe)),
        Intrinsic::CharGt => Direct(ScalarOperation::Binary(BinaryOp::CharGt)),
        Intrinsic::CharGe => Direct(ScalarOperation::Binary(BinaryOp::CharGe)),
        Intrinsic::IntNeg => Direct(ScalarOperation::Unary(UnaryOp::IntNeg)),
        Intrinsic::IntComplement => Direct(ScalarOperation::Unary(UnaryOp::IntComplement)),
        Intrinsic::NumberNeg => Direct(ScalarOperation::Unary(UnaryOp::NumberNeg)),
        Intrinsic::NumberAbs => Direct(ScalarOperation::Unary(UnaryOp::NumberAbs)),
        Intrinsic::NumberSqrt => Direct(ScalarOperation::Unary(UnaryOp::NumberSqrt)),
        Intrinsic::NumberTrunc => Direct(ScalarOperation::Unary(UnaryOp::NumberTrunc)),
        Intrinsic::NumberFloor => Direct(ScalarOperation::Unary(UnaryOp::NumberFloor)),
        Intrinsic::NumberCeil => Direct(ScalarOperation::Unary(UnaryOp::NumberCeil)),
        Intrinsic::BooleanNot => Direct(ScalarOperation::Unary(UnaryOp::BooleanNot)),
        Intrinsic::IntToNumber => Direct(ScalarOperation::Unary(UnaryOp::IntToNumber)),
        Intrinsic::NumberToInt => Direct(ScalarOperation::Unary(UnaryOp::NumberToInt)),
        Intrinsic::BooleanToInt => Direct(ScalarOperation::Unary(UnaryOp::BooleanToInt)),
        Intrinsic::IntToBoolean => Direct(ScalarOperation::Unary(UnaryOp::IntToBoolean)),
        Intrinsic::CharToInt => Direct(ScalarOperation::Unary(UnaryOp::CharToInt)),
        Intrinsic::IntToChar => Direct(ScalarOperation::Unary(UnaryOp::IntToChar)),
        Intrinsic::ArrayLength => Generated(GeneratedOperation::ArrayLength),
        Intrinsic::ArrayIndex => Generated(GeneratedOperation::ArrayIndex),
        Intrinsic::ArrayUpdate => Generated(GeneratedOperation::ArrayUpdate),
        Intrinsic::ArrayAppend => Generated(GeneratedOperation::ArrayAppend),
        Intrinsic::ArrayFill => Generated(GeneratedOperation::ArrayFill),
        Intrinsic::ArrayWrite => Generated(GeneratedOperation::ArrayWrite),
        Intrinsic::StringToBytes => Generated(GeneratedOperation::StringToBytes),
        Intrinsic::BytesToString => Generated(GeneratedOperation::BytesToString),
        Intrinsic::UnsafeCoerce => Generated(GeneratedOperation::UnsafeCoerce),
        Intrinsic::NumberToString => Artifact(&target_runtime::NUMBER_FORMAT),
        Intrinsic::NumberFromDecimal => Artifact(&target_runtime::NUMBER_PARSE),
        Intrinsic::NumberAcos => Artifact(&target_runtime::NUMBER_ACOS),
        Intrinsic::BoolTrue | Intrinsic::BoolFalse | Intrinsic::Unit | Intrinsic::Coerce => {
            Elaborated
        }
        Intrinsic::Undefined => Unsupported,
    }
}

/// Artifact providers are derived from the exhaustive selection, never registered twice.
pub(crate) fn artifacts() -> impl Iterator<Item = &'static ArtifactImplementation> {
    Intrinsic::ALL
        .into_iter()
        .filter_map(|intrinsic| match implementation(intrinsic) {
            Implementation::Artifact(provider) => Some(provider),
            _ => None,
        })
}

#[cfg(test)]
mod tests;
