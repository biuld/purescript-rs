//! The compiler's built-in value vocabulary.
//!
//! This is the one authoritative identity list for compiler-known values: every
//! stage keys off `Intrinsic` and its stable [`Intrinsic::symbol`], so no stage
//! keeps a second name table. The metadata beside that identity — name, arity,
//! category, and type scheme — lives in [`registry`], and each pass interprets
//! it in its own representation.

mod effects;
mod registry;

pub use effects::IntrinsicEffects;
pub use registry::{IntrinsicCategory, IntrinsicDescriptor};

use crate::{ModuleId, SymbolId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Intrinsic {
    BoolTrue = 0,
    BoolFalse = 1,
    IntAdd = 2,
    IntSub = 3,
    IntMul = 4,
    IntQuot = 5,
    IntRem = 6,
    IntEq = 7,
    IntNe = 8,
    IntLt = 9,
    IntLe = 10,
    IntGt = 11,
    IntGe = 12,
    ArrayLength = 13,
    ArrayIndex = 14,
    ArrayUpdate = 15,
    IntNeg = 16,
    IntComplement = 17,
    NumberNeg = 18,
    BooleanNot = 19,
    IntToNumber = 20,
    NumberToInt = 21,
    BooleanToInt = 22,
    IntToBoolean = 23,
    CharToInt = 24,
    IntToChar = 25,
    IntAnd = 28,
    IntOr = 29,
    IntXor = 30,
    IntShl = 31,
    IntShr = 32,
    IntZshr = 33,
    NumberAdd = 34,
    NumberSub = 35,
    NumberMul = 36,
    NumberDiv = 37,
    NumberEq = 38,
    NumberNe = 39,
    NumberLt = 40,
    NumberLe = 41,
    NumberGt = 42,
    NumberGe = 43,
    BooleanAnd = 44,
    BooleanOr = 45,
    BooleanEq = 46,
    BooleanNe = 47,
    CharEq = 48,
    CharNe = 49,
    CharLt = 50,
    CharLe = 51,
    CharGt = 52,
    CharGe = 53,
    /// A source `String`'s canonical UTF-8 bytes as an `Array Int`. A source
    /// string is a sequence of Unicode scalar values, so this is lossless and
    /// never fails
    /// ([DEC-16](../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
    StringToBytes = 54,
    /// An `Array Int` as a source `String`. Each element must be a canonical
    /// byte and the bytes must be well-formed UTF-8; either violation traps
    /// rather than producing replacement text.
    BytesToString = 55,
    /// Source-level `Safe.Coerce.coerce`, elaborated to a checked coercion.
    Coerce = 56,
    /// The compiler-provided partial value `Prim.undefined`, whose type is
    /// `forall a. a`. It has no runtime representation yet, so the stages that
    /// would have to choose one report it instead of inventing it.
    Undefined = 57,
    /// The one `Unit` value, written `unit` or `()`. A compiler primitive rather
    /// than a nullary constructor, because `Unit` is a builtin type here.
    Unit = 58,
    /// Source-level `Array.append`: concatenates two arrays of the same element
    /// type into a fresh array. A compiler primitive because building an array
    /// of a computed length has no source spelling; the library's
    /// `Semigroup (Array a)` instance and `Semigroup String` are its users.
    ArrayAppend = 59,
    /// Source-level `Unsafe.Coerce.unsafeCoerce`, the unchecked representation
    /// coercion (`unsafeCoerce#`). Unlike `Coerce` it carries no `Coercible`
    /// proof; it is a representation-preserving cast at the value's erased
    /// boundary.
    UnsafeCoerce = 60,
    /// Allocate a fresh array fully initialized with one checked element.
    ArrayFill = 61,
    /// Unsafe in-place write; returns the same array. Library internals only.
    ArrayWrite = 62,
    NumberToString = 63,
    /// Truncate an IEEE-754 Number toward zero, retaining its Number representation.
    NumberTrunc = 64,
    /// Round a Number toward negative infinity.
    NumberFloor = 65,
    /// Round a Number toward positive infinity.
    NumberCeil = 66,
    /// Convert a complete ASCII decimal token to binary64; invalid tokens return NaN.
    NumberFromDecimal = 67,
    /// Clear the binary64 sign bit, including negative zero and NaN.
    NumberAbs = 68,
    /// IEEE-754 square root. Negative finite inputs and NaN produce NaN.
    /// Negative zero remains negative zero. The operation does not trap.
    NumberSqrt = 69,
    /// Inverse cosine in radians. Finite inputs outside [-1, 1], infinities,
    /// and NaN produce NaN. The operation does not trap.
    NumberAcos = 70,
    /// Inverse sine in radians. Finite inputs outside [-1, 1], infinities,
    /// and NaN produce NaN. Negative zero remains negative zero. The operation
    /// does not trap.
    NumberAsin = 71,
    /// Inverse tangent in radians. Positive and negative infinity produce
    /// positive and negative pi/2. Negative zero remains negative zero. NaN
    /// produces NaN. The operation does not trap.
    NumberAtan = 72,
}

impl Intrinsic {
    pub const fn symbol(self) -> SymbolId {
        SymbolId::new(ModuleId::INTRINSICS, self as u32)
    }

    /// The metadata that describes this intrinsic: its name, arity, category, and
    /// type scheme. See [`IntrinsicDescriptor`].
    pub fn descriptor(self) -> IntrinsicDescriptor {
        registry::descriptor(self)
    }

    /// Resolves an explicit primitive binding using the authoritative registry.
    pub fn from_binding(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|value| value.descriptor().name == name)
    }

    /// Retired floor-division identities; these slots must never be reused.
    pub const RESERVED_IDS: [u32; 2] = [26, 27];

    /// Every active variant, in discriminant order. `bootstrap_externals` builds the
    /// compiler-known externals from it; the assertion below keeps it exact.
    pub const ALL: [Intrinsic; 71] = [
        Intrinsic::BoolTrue,
        Intrinsic::BoolFalse,
        Intrinsic::IntAdd,
        Intrinsic::IntSub,
        Intrinsic::IntMul,
        Intrinsic::IntQuot,
        Intrinsic::IntRem,
        Intrinsic::IntEq,
        Intrinsic::IntNe,
        Intrinsic::IntLt,
        Intrinsic::IntLe,
        Intrinsic::IntGt,
        Intrinsic::IntGe,
        Intrinsic::ArrayLength,
        Intrinsic::ArrayIndex,
        Intrinsic::ArrayUpdate,
        Intrinsic::IntNeg,
        Intrinsic::IntComplement,
        Intrinsic::NumberNeg,
        Intrinsic::BooleanNot,
        Intrinsic::IntToNumber,
        Intrinsic::NumberToInt,
        Intrinsic::BooleanToInt,
        Intrinsic::IntToBoolean,
        Intrinsic::CharToInt,
        Intrinsic::IntToChar,
        Intrinsic::IntAnd,
        Intrinsic::IntOr,
        Intrinsic::IntXor,
        Intrinsic::IntShl,
        Intrinsic::IntShr,
        Intrinsic::IntZshr,
        Intrinsic::NumberAdd,
        Intrinsic::NumberSub,
        Intrinsic::NumberMul,
        Intrinsic::NumberDiv,
        Intrinsic::NumberEq,
        Intrinsic::NumberNe,
        Intrinsic::NumberLt,
        Intrinsic::NumberLe,
        Intrinsic::NumberGt,
        Intrinsic::NumberGe,
        Intrinsic::BooleanAnd,
        Intrinsic::BooleanOr,
        Intrinsic::BooleanEq,
        Intrinsic::BooleanNe,
        Intrinsic::CharEq,
        Intrinsic::CharNe,
        Intrinsic::CharLt,
        Intrinsic::CharLe,
        Intrinsic::CharGt,
        Intrinsic::CharGe,
        Intrinsic::StringToBytes,
        Intrinsic::BytesToString,
        Intrinsic::Coerce,
        Intrinsic::Undefined,
        Intrinsic::Unit,
        Intrinsic::ArrayAppend,
        Intrinsic::UnsafeCoerce,
        Intrinsic::ArrayFill,
        Intrinsic::ArrayWrite,
        Intrinsic::NumberToString,
        Intrinsic::NumberTrunc,
        Intrinsic::NumberFloor,
        Intrinsic::NumberCeil,
        Intrinsic::NumberFromDecimal,
        Intrinsic::NumberAbs,
        Intrinsic::NumberSqrt,
        Intrinsic::NumberAcos,
        Intrinsic::NumberAsin,
        Intrinsic::NumberAtan,
    ];
}

// Active and reserved identities cover every stable discriminant exactly once.
// Retired identities cannot be reused by a later operation. A new variant therefore
// cannot be added and silently left out of the bootstrap name table.
const _: () = {
    assert!(
        Intrinsic::ALL.len() + Intrinsic::RESERVED_IDS.len()
            == Intrinsic::NumberAtan as u32 as usize + 1,
        "Intrinsic::ALL is out of date: update it when adding a variant",
    );
    let mut seen: u128 =
        (1u128 << Intrinsic::RESERVED_IDS[0]) | (1u128 << Intrinsic::RESERVED_IDS[1]);
    let mut index = 0;
    while index < Intrinsic::ALL.len() {
        let bit = 1u128 << (Intrinsic::ALL[index] as u32);
        assert!(seen & bit == 0, "Intrinsic::ALL repeats a variant");
        seen |= bit;
        index += 1;
    }
};
