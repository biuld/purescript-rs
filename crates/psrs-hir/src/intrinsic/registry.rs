//! Metadata for each [`Intrinsic`]: its name, arity, category, and type scheme.
//!
//! This module is data, not lowering. Every pass reads the same descriptor and
//! interprets it in its own representation; the registry depends only on
//! `psrs-hir`, so no pass gains a dependency and no cycle is possible.
//!
//! Adding a variant to [`Intrinsic`](super::Intrinsic) forces an entry here
//! through the exhaustive `descriptor` match, so a variant cannot be registered
//! silently.

use super::{Intrinsic, IntrinsicEffects};
use crate::{BuiltinType, Type, TypeField, TypeId, TypeKind, TypeParameter};
use psrs_span::TextRange;

/// A representation-independent classification of an intrinsic: what kind of
/// term it is, not how any one stage emits it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntrinsicCategory {
    /// A nullary value: `true`, `false`, `unit`, or a numeric constant.
    Nullary,
    /// One language argument and a language result.
    Unary,
    /// Two scalar arguments and a scalar result.
    BinaryScalar,
    /// `Array.length`: `forall a. Array a -> Int`.
    ArrayLength,
    /// `Array.index`: `forall a. Array a -> Int -> a`.
    ArrayIndex,
    /// `Array.update`: `forall a. Array a -> Int -> a -> Array a`.
    ArrayUpdate,
    /// `Array.append`: `forall a. Array a -> Array a -> Array a`.
    ArrayAppend,
    /// `forall a. Int -> a -> Array a`.
    ArrayFill,
    /// `forall a. Array a -> Int -> a -> Array a`, in place.
    ArrayWrite,
    /// A source `String` to its canonical UTF-8 bytes.
    StringToBytes,
    /// Canonical UTF-8 bytes back to a source `String`.
    BytesToString,
    /// `Safe.Coerce.coerce`: checked evidence with no runtime operation of its
    /// own.
    Coercion,
    /// `Prim.undefined`: a value with no runtime representation.
    PartialValue,
    /// A compiler-owned delimited-control primitive: `Control`'s
    /// `return`/`bind`/`perform`/`run`.
    Control,
}

/// The metadata for one intrinsic.
#[derive(Clone, Copy, Debug)]
pub struct IntrinsicDescriptor {
    pub intrinsic: Intrinsic,
    /// The name `resolve` binds for this intrinsic. It is the compiler-internal
    /// primitive name. Surface operator spellings and their fixities belong to
    /// the library (`Data.Ring` owns `-`, `Data.EuclideanRing` owns `/`).
    /// `%` is still the truncating remainder primitive.
    pub name: &'static str,
    /// The number of arguments a saturated call supplies: the leading-arrow
    /// count of `scheme`.
    pub arity: u8,
    pub category: IntrinsicCategory,
    pub effects: IntrinsicEffects,
    /// A non-capturing constructor, so the descriptor table can be `const`.
    pub scheme: fn() -> Type,
}

/// Builds [`descriptor`] from one line per intrinsic. The generated `match` is
/// exhaustive, so a variant with no entry is a compile error.
macro_rules! descriptors {
    ($( $variant:ident => $name:expr, $arity:expr, $category:ident, $scheme:path; )*) => {
        /// The descriptor for an intrinsic. The match is exhaustive over
        /// [`Intrinsic`], so every variant has exactly one entry.
        pub fn descriptor(intrinsic: Intrinsic) -> IntrinsicDescriptor {
            match intrinsic {
                $(
                    Intrinsic::$variant => IntrinsicDescriptor {
                        intrinsic: Intrinsic::$variant,
                        name: $name,
                        arity: $arity,
                        category: IntrinsicCategory::$category,
                        effects: IntrinsicEffects::for_intrinsic(intrinsic),
                        scheme: $scheme,
                    },
                )*
            }
        }
    };
}

descriptors! {
    BoolTrue => "true", 0, Nullary, scheme::boolean;
    BoolFalse => "false", 0, Nullary, scheme::boolean;
    IntAdd => "intAdd", 2, BinaryScalar, scheme::int_int_int;
    IntSub => "intSub", 2, BinaryScalar, scheme::int_int_int;
    IntMul => "intMul", 2, BinaryScalar, scheme::int_int_int;
    IntQuot => "intQuot", 2, BinaryScalar, scheme::int_int_int;
    IntRem => "%", 2, BinaryScalar, scheme::int_int_int;
    IntEq => "intEq", 2, BinaryScalar, scheme::int_int_bool;
    IntNe => "intNe", 2, BinaryScalar, scheme::int_int_bool;
    IntLt => "intLt", 2, BinaryScalar, scheme::int_int_bool;
    IntLe => "intLe", 2, BinaryScalar, scheme::int_int_bool;
    IntGt => "intGt", 2, BinaryScalar, scheme::int_int_bool;
    IntGe => "intGe", 2, BinaryScalar, scheme::int_int_bool;
    ArrayLength => "arrayLength", 1, ArrayLength, scheme::array_length;
    ArrayIndex => "arrayIndex", 2, ArrayIndex, scheme::array_index;
    ArrayUpdate => "arrayUpdate", 3, ArrayUpdate, scheme::array_update;
    IntNeg => "intNeg", 1, Unary, scheme::int_int;
    IntComplement => "intComplement", 1, Unary, scheme::int_int;
    NumberNeg => "numberNeg", 1, Unary, scheme::number_number;
    NumberTrunc => "numberTrunc", 1, Unary, scheme::number_number;
    NumberFloor => "numberFloor", 1, Unary, scheme::number_number;
    NumberAbs => "numberAbs", 1, Unary, scheme::number_number;
    NumberSqrt => "numberSqrt", 1, Unary, scheme::number_number;
    NumberAcos => "numberAcos", 1, Unary, scheme::number_number;
    NumberAsin => "numberAsin", 1, Unary, scheme::number_number;
    NumberAtan => "numberAtan", 1, Unary, scheme::number_number;
    NumberAtan2 => "numberAtan2", 2, BinaryScalar, scheme::number_number_number;
    NumberSin => "numberSin", 1, Unary, scheme::number_number;
    NumberCos => "numberCos", 1, Unary, scheme::number_number;
    NumberTan => "numberTan", 1, Unary, scheme::number_number;
    NumberExp => "numberExp", 1, Unary, scheme::number_number;
    NumberLog => "numberLog", 1, Unary, scheme::number_number;
    NumberPow => "numberPow", 2, BinaryScalar, scheme::number_number_number;
    NumberMin => "numberMin", 2, BinaryScalar, scheme::number_number_number;
    NumberMax => "numberMax", 2, BinaryScalar, scheme::number_number_number;
    NumberSign => "numberSign", 1, Unary, scheme::number_number;
    NumberRemainder => "numberRemainder", 2, BinaryScalar, scheme::number_number_number;
    NumberIsNaN => "numberIsNaN", 1, Unary, scheme::number_bool;
    NumberNaN => "numberNan", 0, Nullary, scheme::number;
    NumberInfinity => "numberInfinity", 0, Nullary, scheme::number;
    NumberCeil => "numberCeil", 1, Unary, scheme::number_number;
    BooleanNot => "booleanNot", 1, Unary, scheme::boolean_boolean;
    IntToNumber => "intToNumber", 1, Unary, scheme::int_number;
    NumberToInt => "numberToInt", 1, Unary, scheme::number_int;
    BooleanToInt => "booleanToInt", 1, Unary, scheme::boolean_int;
    IntToBoolean => "intToBoolean", 1, Unary, scheme::int_boolean;
    CharToInt => "charToInt", 1, Unary, scheme::char_int;
    IntToChar => "intToChar", 1, Unary, scheme::int_char;
    IntAnd => "intAnd", 2, BinaryScalar, scheme::int_int_int;
    IntOr => "intOr", 2, BinaryScalar, scheme::int_int_int;
    IntXor => "intXor", 2, BinaryScalar, scheme::int_int_int;
    IntShl => "intShl", 2, BinaryScalar, scheme::int_int_int;
    IntShr => "intShr", 2, BinaryScalar, scheme::int_int_int;
    IntZshr => "intZshr", 2, BinaryScalar, scheme::int_int_int;
    NumberAdd => "numberAdd", 2, BinaryScalar, scheme::number_number_number;
    NumberSub => "numberSub", 2, BinaryScalar, scheme::number_number_number;
    NumberMul => "numberMul", 2, BinaryScalar, scheme::number_number_number;
    NumberDiv => "numberDiv", 2, BinaryScalar, scheme::number_number_number;
    NumberEq => "numberEq", 2, BinaryScalar, scheme::number_number_bool;
    NumberNe => "numberNe", 2, BinaryScalar, scheme::number_number_bool;
    NumberLt => "numberLt", 2, BinaryScalar, scheme::number_number_bool;
    NumberLe => "numberLe", 2, BinaryScalar, scheme::number_number_bool;
    NumberGt => "numberGt", 2, BinaryScalar, scheme::number_number_bool;
    NumberGe => "numberGe", 2, BinaryScalar, scheme::number_number_bool;
    BooleanAnd => "booleanAnd", 2, BinaryScalar, scheme::boolean_boolean_boolean;
    BooleanOr => "booleanOr", 2, BinaryScalar, scheme::boolean_boolean_boolean;
    BooleanEq => "booleanEq", 2, BinaryScalar, scheme::boolean_boolean_boolean;
    BooleanNe => "booleanNe", 2, BinaryScalar, scheme::boolean_boolean_boolean;
    CharEq => "charEq", 2, BinaryScalar, scheme::char_char_bool;
    CharNe => "charNe", 2, BinaryScalar, scheme::char_char_bool;
    CharLt => "charLt", 2, BinaryScalar, scheme::char_char_bool;
    CharLe => "charLe", 2, BinaryScalar, scheme::char_char_bool;
    CharGt => "charGt", 2, BinaryScalar, scheme::char_char_bool;
    CharGe => "charGe", 2, BinaryScalar, scheme::char_char_bool;
    StringToBytes => "stringToBytes", 1, StringToBytes, scheme::string_to_bytes;
    BytesToString => "bytesToString", 1, BytesToString, scheme::bytes_to_string;
    Coerce => "__psrs_coerce", 1, Coercion, scheme::coerce;
    Undefined => "__psrs_undefined", 0, PartialValue, scheme::undefined;
    Unit => "unit", 0, Nullary, scheme::unit;
    ArrayAppend => "arrayAppend", 2, ArrayAppend, scheme::array_append;
    UnsafeCoerce => "__psrs_unsafe_coerce", 1, Coercion, scheme::unsafe_coerce;
    ArrayFill => "arrayFill", 2, ArrayFill, scheme::array_fill;
    ArrayWrite => "arrayWrite", 3, ArrayWrite, scheme::array_update;
    NumberToString => "numberToString", 1, Unary, scheme::number_string;
    NumberFromDecimal => "numberFromDecimal", 1, Unary, scheme::string_number;
    ControlReturn => "controlReturn", 1, Control, scheme::control_return;
    ControlBind => "controlBind", 2, Control, scheme::control_bind;
    ControlPerform => "controlPerform", 1, Control, scheme::control_perform;
    ControlRun => "controlRun", 3, Control, scheme::control_run;
    ControlHandle => "controlHandle", 5, Control, scheme::control_handle;
    ControlOnce => "controlOnce", 1, Control, scheme::control_once;
    ControlResume => "controlResume", 2, Control, scheme::control_resume;
    ControlCancel => "controlCancel", 1, Control, scheme::control_cancel;
}

/// The HIR type schemes. Each returns a fresh [`Type`], so a caller that
/// instantiates it at a use site gets its own type variables.
mod scheme;

#[cfg(test)]
mod tests {
    use super::*;

    /// `arity` is the leading-arrow count of `scheme`, so the two cannot drift.
    #[test]
    fn arity_matches_the_scheme() {
        for intrinsic in Intrinsic::ALL {
            let descriptor = intrinsic.descriptor();
            assert_eq!(
                descriptor.arity as usize,
                leading_arrows(&(descriptor.scheme)()),
                "arity disagrees with the scheme for {intrinsic:?}",
            );
            assert_eq!(descriptor.intrinsic, intrinsic);
        }
    }

    fn leading_arrows(ty: &Type) -> usize {
        match &ty.kind {
            TypeKind::Forall { body, .. } | TypeKind::Constrained { body, .. } => {
                leading_arrows(body)
            }
            TypeKind::Function { result, .. } => 1 + leading_arrows(result),
            _ => 0,
        }
    }
}
