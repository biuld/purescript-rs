//! Metadata for each [`Intrinsic`]: its name, arity, category, and type scheme.
//!
//! This module is data, not lowering. Every pass reads the same descriptor and
//! interprets it in its own representation; the registry depends only on
//! `psrs-hir`, so no pass gains a dependency and no cycle is possible.
//!
//! Adding a variant to [`Intrinsic`](super::Intrinsic) forces an entry here
//! through the exhaustive `descriptor` match, so a variant cannot be registered
//! silently.

use super::Intrinsic;
use crate::{BuiltinType, Type, TypeId, TypeKind, TypeParameter};
use psrs_span::TextRange;

/// A representation-independent classification of an intrinsic: what kind of
/// term it is, not how any one stage emits it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntrinsicCategory {
    /// A nullary value: `true`, `false`, `unit`.
    Nullary,
    /// One scalar argument and a scalar result.
    UnaryScalar,
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
    I32Add => "intAdd", 2, BinaryScalar, scheme::int_int_int;
    I32Sub => "intSub", 2, BinaryScalar, scheme::int_int_int;
    I32Mul => "intMul", 2, BinaryScalar, scheme::int_int_int;
    I32DivS => "intQuot", 2, BinaryScalar, scheme::int_int_int;
    I32RemS => "%", 2, BinaryScalar, scheme::int_int_int;
    I32Eq => "intEq", 2, BinaryScalar, scheme::int_int_bool;
    I32Ne => "intNe", 2, BinaryScalar, scheme::int_int_bool;
    I32LtS => "intLt", 2, BinaryScalar, scheme::int_int_bool;
    I32LeS => "intLe", 2, BinaryScalar, scheme::int_int_bool;
    I32GtS => "intGt", 2, BinaryScalar, scheme::int_int_bool;
    I32GeS => "intGe", 2, BinaryScalar, scheme::int_int_bool;
    ArrayLength => "arrayLength", 1, ArrayLength, scheme::array_length;
    ArrayIndex => "arrayIndex", 2, ArrayIndex, scheme::array_index;
    ArrayUpdate => "arrayUpdate", 3, ArrayUpdate, scheme::array_update;
    IntNeg => "intNeg", 1, UnaryScalar, scheme::int_int;
    IntComplement => "intComplement", 1, UnaryScalar, scheme::int_int;
    NumberNeg => "numberNeg", 1, UnaryScalar, scheme::number_number;
    NumberTrunc => "numberTrunc", 1, UnaryScalar, scheme::number_number;
    NumberFloor => "numberFloor", 1, UnaryScalar, scheme::number_number;
    NumberCeil => "numberCeil", 1, UnaryScalar, scheme::number_number;
    BooleanNot => "booleanNot", 1, UnaryScalar, scheme::boolean_boolean;
    IntToNumber => "intToNumber", 1, UnaryScalar, scheme::int_number;
    NumberToInt => "numberToInt", 1, UnaryScalar, scheme::number_int;
    BooleanToInt => "booleanToInt", 1, UnaryScalar, scheme::boolean_int;
    IntToBoolean => "intToBoolean", 1, UnaryScalar, scheme::int_boolean;
    CharToInt => "charToInt", 1, UnaryScalar, scheme::char_int;
    IntToChar => "intToChar", 1, UnaryScalar, scheme::int_char;
    IntDiv => "intDiv", 2, BinaryScalar, scheme::int_int_int;
    IntMod => "intMod", 2, BinaryScalar, scheme::int_int_int;
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
    NumberToString => "numberToString", 1, UnaryScalar, scheme::number_string;
}

/// The HIR type schemes. Each returns a fresh [`Type`], so a caller that
/// instantiates it at a use site gets its own type variables.
mod scheme {
    use super::*;

    fn empty_span() -> TextRange {
        TextRange::new(0, 0)
    }

    fn builtin(builtin: BuiltinType) -> Type {
        Type {
            kind: TypeKind::Constructor(builtin),
            span: empty_span(),
        }
    }

    fn named(id: TypeId) -> Type {
        Type {
            kind: TypeKind::Named(id),
            span: empty_span(),
        }
    }

    fn variable(name: &str) -> Type {
        Type {
            kind: TypeKind::Variable(name.to_owned()),
            span: empty_span(),
        }
    }

    fn apply(function: Type, argument: Type) -> Type {
        Type {
            kind: TypeKind::Application(Box::new(function), Box::new(argument)),
            span: empty_span(),
        }
    }

    fn arrow(parameter: Type, result: Type) -> Type {
        Type {
            kind: TypeKind::Function {
                parameter: Box::new(parameter),
                result: Box::new(result),
            },
            span: empty_span(),
        }
    }

    fn forall(names: &[&str], body: Type) -> Type {
        Type {
            kind: TypeKind::Forall {
                variables: names
                    .iter()
                    .map(|name| TypeParameter {
                        name: (*name).to_owned(),
                        name_span: empty_span(),
                        kind: None,
                    })
                    .collect(),
                body: Box::new(body),
            },
            span: empty_span(),
        }
    }

    fn array(element: Type) -> Type {
        apply(builtin(BuiltinType::Array), element)
    }

    fn int() -> Type {
        builtin(BuiltinType::Int)
    }

    fn scalar(operand: BuiltinType, result: BuiltinType) -> Type {
        let operand = builtin(operand);
        arrow(operand.clone(), arrow(operand, builtin(result)))
    }

    fn unary(operand: BuiltinType, result: BuiltinType) -> Type {
        arrow(builtin(operand), builtin(result))
    }

    pub(super) fn boolean() -> Type {
        builtin(BuiltinType::Boolean)
    }

    pub(super) fn unit() -> Type {
        builtin(BuiltinType::Unit)
    }

    pub(super) fn int_int_int() -> Type {
        scalar(BuiltinType::Int, BuiltinType::Int)
    }

    pub(super) fn int_int_bool() -> Type {
        scalar(BuiltinType::Int, BuiltinType::Boolean)
    }

    pub(super) fn number_number_number() -> Type {
        scalar(BuiltinType::Number, BuiltinType::Number)
    }

    pub(super) fn number_number_bool() -> Type {
        scalar(BuiltinType::Number, BuiltinType::Boolean)
    }

    pub(super) fn boolean_boolean_boolean() -> Type {
        scalar(BuiltinType::Boolean, BuiltinType::Boolean)
    }

    pub(super) fn char_char_bool() -> Type {
        scalar(BuiltinType::Char, BuiltinType::Boolean)
    }

    pub(super) fn int_int() -> Type {
        unary(BuiltinType::Int, BuiltinType::Int)
    }

    pub(super) fn number_number() -> Type {
        unary(BuiltinType::Number, BuiltinType::Number)
    }

    pub(super) fn boolean_boolean() -> Type {
        unary(BuiltinType::Boolean, BuiltinType::Boolean)
    }

    pub(super) fn int_number() -> Type {
        unary(BuiltinType::Int, BuiltinType::Number)
    }

    pub(super) fn number_string() -> Type {
        unary(BuiltinType::Number, BuiltinType::String)
    }

    pub(super) fn number_int() -> Type {
        unary(BuiltinType::Number, BuiltinType::Int)
    }

    pub(super) fn boolean_int() -> Type {
        unary(BuiltinType::Boolean, BuiltinType::Int)
    }

    pub(super) fn int_boolean() -> Type {
        unary(BuiltinType::Int, BuiltinType::Boolean)
    }

    pub(super) fn char_int() -> Type {
        unary(BuiltinType::Char, BuiltinType::Int)
    }

    pub(super) fn int_char() -> Type {
        unary(BuiltinType::Int, BuiltinType::Char)
    }

    pub(super) fn array_length() -> Type {
        forall(&["a"], arrow(array(variable("a")), int()))
    }

    pub(super) fn array_index() -> Type {
        forall(
            &["a"],
            arrow(array(variable("a")), arrow(int(), variable("a"))),
        )
    }

    pub(super) fn array_update() -> Type {
        forall(
            &["a"],
            arrow(
                array(variable("a")),
                arrow(int(), arrow(variable("a"), array(variable("a")))),
            ),
        )
    }

    pub(super) fn array_append() -> Type {
        forall(
            &["a"],
            arrow(
                array(variable("a")),
                arrow(array(variable("a")), array(variable("a"))),
            ),
        )
    }

    pub(super) fn array_fill() -> Type {
        forall(
            &["a"],
            arrow(int(), arrow(variable("a"), array(variable("a")))),
        )
    }

    pub(super) fn string_to_bytes() -> Type {
        arrow(builtin(BuiltinType::String), array(int()))
    }

    pub(super) fn bytes_to_string() -> Type {
        arrow(array(int()), builtin(BuiltinType::String))
    }

    pub(super) fn undefined() -> Type {
        forall(&["a"], variable("a"))
    }

    pub(super) fn coerce() -> Type {
        let constraint = apply(
            apply(named(TypeId::COERCIBLE), variable("a")),
            variable("b"),
        );
        Type {
            kind: TypeKind::Constrained {
                constraint: Box::new(constraint),
                body: Box::new(arrow(variable("a"), variable("b"))),
            },
            span: empty_span(),
        }
    }

    /// `Unsafe.Coerce.unsafeCoerce`: an unconstrained identity cast. It has no
    /// `Coercible` proof; the value crosses its erased representation unchanged.
    pub(super) fn unsafe_coerce() -> Type {
        forall(&["a", "b"], arrow(variable("a"), variable("b")))
    }
}

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
