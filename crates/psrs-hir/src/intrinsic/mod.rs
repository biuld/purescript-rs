//! The compiler's built-in value vocabulary.
//!
//! This is the one authoritative identity list for compiler-known values: every
//! stage keys off `Intrinsic` and its stable [`Intrinsic::symbol`], so no stage
//! keeps a second name table. The metadata beside that identity — name, arity,
//! category, and type scheme — lives in [`registry`], and each pass interprets
//! it in its own representation.

mod registry;

pub use registry::{IntrinsicCategory, IntrinsicDescriptor};

use crate::{ModuleId, SymbolId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Intrinsic {
    BoolTrue,
    BoolFalse,
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32RemS,
    I32Eq,
    I32Ne,
    I32LtS,
    I32LeS,
    I32GtS,
    I32GeS,
    ArrayLength,
    ArrayIndex,
    ArrayUpdate,
    IntNeg,
    IntComplement,
    NumberNeg,
    BooleanNot,
    IntToNumber,
    NumberToInt,
    BooleanToInt,
    IntToBoolean,
    CharToInt,
    IntToChar,
    IntDiv,
    IntMod,
    IntAnd,
    IntOr,
    IntXor,
    IntShl,
    IntShr,
    IntZshr,
    NumberAdd,
    NumberSub,
    NumberMul,
    NumberDiv,
    NumberEq,
    NumberNe,
    NumberLt,
    NumberLe,
    NumberGt,
    NumberGe,
    BooleanAnd,
    BooleanOr,
    BooleanEq,
    BooleanNe,
    CharEq,
    CharNe,
    CharLt,
    CharLe,
    CharGt,
    CharGe,
    /// A source `String`'s canonical UTF-8 bytes as an `Array Int`. A source
    /// string is a sequence of Unicode scalar values, so this is lossless and
    /// never fails
    /// ([DEC-16](../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).
    StringToBytes,
    /// An `Array Int` as a source `String`. Each element must be a canonical
    /// byte and the bytes must be well-formed UTF-8; either violation traps
    /// rather than producing replacement text.
    BytesToString,
    /// Source-level `Safe.Coerce.coerce`, elaborated to a checked coercion.
    Coerce,
    /// The compiler-provided partial value `Prim.undefined`, whose type is
    /// `forall a. a`. It has no runtime representation yet, so the stages that
    /// would have to choose one report it instead of inventing it.
    Undefined,
    /// The one `Unit` value, written `unit` or `()`. A compiler primitive rather
    /// than a nullary constructor, because `Unit` is a builtin type here.
    Unit,
    /// Source-level `Array.append`: concatenates two arrays of the same element
    /// type into a fresh array. A compiler primitive because building an array
    /// of a computed length has no source spelling; the library's
    /// `Semigroup (Array a)` instance and `Semigroup String` are its users.
    ArrayAppend,
    /// Source-level `Unsafe.Coerce.unsafeCoerce`, the unchecked representation
    /// coercion (`unsafeCoerce#`). Unlike `Coerce` it carries no `Coercible`
    /// proof; it is a representation-preserving cast at the value's erased
    /// boundary.
    UnsafeCoerce,
    /// Allocate a fresh array fully initialized with one checked element.
    ArrayFill,
    /// Unsafe in-place write; returns the same array. Library internals only.
    ArrayWrite,
    NumberToString,
    /// Truncate an IEEE-754 Number toward zero, retaining its Number representation.
    NumberTrunc,
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

    /// Every variant, in discriminant order. `bootstrap_externals` builds the
    /// compiler-known externals from it; the assertion below keeps it exact.
    pub const ALL: [Intrinsic; 65] = [
        Intrinsic::BoolTrue,
        Intrinsic::BoolFalse,
        Intrinsic::I32Add,
        Intrinsic::I32Sub,
        Intrinsic::I32Mul,
        Intrinsic::I32DivS,
        Intrinsic::I32RemS,
        Intrinsic::I32Eq,
        Intrinsic::I32Ne,
        Intrinsic::I32LtS,
        Intrinsic::I32LeS,
        Intrinsic::I32GtS,
        Intrinsic::I32GeS,
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
        Intrinsic::IntDiv,
        Intrinsic::IntMod,
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
    ];
}

// `ALL` lists every discriminant exactly once: its length is the highest
// discriminant plus one, and the values are unique. A new variant therefore
// cannot be added and silently left out of the bootstrap name table.
const _: () = {
    assert!(
        Intrinsic::ALL.len() == Intrinsic::NumberTrunc as u32 as usize + 1,
        "Intrinsic::ALL is out of date: update it when adding a variant",
    );
    let mut seen: u128 = 0;
    let mut index = 0;
    while index < Intrinsic::ALL.len() {
        let bit = 1u128 << (Intrinsic::ALL[index] as u32);
        assert!(seen & bit == 0, "Intrinsic::ALL repeats a variant");
        seen |= bit;
        index += 1;
    }
};
