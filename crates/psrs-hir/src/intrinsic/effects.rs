//! Conservative semantic effects, independent of a target provider.
use super::Intrinsic;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IntrinsicEffects {
    pub may_trap: bool,
    pub may_write: bool,
}

impl IntrinsicEffects {
    pub(super) fn for_intrinsic(intrinsic: Intrinsic) -> Self {
        use Intrinsic::*;
        match intrinsic {
            ArrayWrite => Self {
                may_trap: true,
                may_write: true,
            },
            IntQuot | IntRem | ArrayIndex | ArrayUpdate | StringToBytes | BytesToString
            | Undefined | ArrayAppend | UnsafeCoerce | ArrayFill | NumberToString
            | NumberFromDecimal => Self {
                may_trap: true,
                may_write: false,
            },
            BoolTrue | BoolFalse | IntAdd | IntSub | IntMul | IntEq | IntNe | IntLt | IntLe
            | IntGt | IntGe | ArrayLength | IntNeg | IntComplement | NumberNeg | BooleanNot
            | IntToNumber | NumberToInt | BooleanToInt | IntToBoolean | CharToInt | IntToChar
            | IntAnd | IntOr | IntXor | IntShl | IntShr | IntZshr | NumberAdd | NumberSub
            | NumberMul | NumberDiv | NumberEq | NumberNe | NumberLt | NumberLe | NumberGt
            | NumberGe | BooleanAnd | BooleanOr | BooleanEq | BooleanNe | CharEq | CharNe
            | CharLt | CharLe | CharGt | CharGe | Coerce | Unit | NumberTrunc | NumberFloor
            | NumberCeil | NumberAbs | NumberSqrt => Self::default(),
        }
    }
}
