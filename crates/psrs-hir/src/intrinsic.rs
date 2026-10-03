//! The compiler's built-in value vocabulary.
//!
//! This is the one authoritative identity list for compiler-known values: every
//! stage keys off `Intrinsic` and its stable [`Intrinsic::symbol`], so no stage
//! keeps a second name table.

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
}

impl Intrinsic {
    pub const fn symbol(self) -> SymbolId {
        SymbolId::new(ModuleId::INTRINSICS, self as u32)
    }
}
