//! Element layout for a non-byte canonical `list<T>`.
//!
//! `list<u8>` and `string` stay [`super::WasiParamKind::List`]. Every other
//! supported element is copied between a source array and a `(pointer, length)`
//! list. Records, handles, nested lists, and `option` are not elements.

use super::WasiParamKind;

/// One already-lowered element of a non-byte list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListElement {
    /// `s32`, `u32`, or `char`: four bytes, GC `i32`.
    Word,
    /// `s8`/`u8`/`s16`/`u16` stored at its canonical width. `u8` lists are not
    /// this variant; those stay strings.
    Narrow { bits: u8, signed: bool },
    /// `bool`: one byte, GC `i32` `0`/`1`.
    Boolean,
    /// `s64`/`u64`: eight bytes, wrapped to the GC `i32` `Int`.
    Scalar64 { signed: bool },
    /// `f32`: four bytes, widened to the GC `f64` `Number`.
    Float32,
    /// `f64`: eight bytes, the GC `Number`.
    Float64,
    /// `string` or `list<u8>`: eight-byte `(pointer, length)`, GC string.
    String,
}

/// The list element a supported parameter kind lowers, if it is one.
pub(crate) fn from_param(kind: &WasiParamKind) -> Option<ListElement> {
    Some(match kind {
        WasiParamKind::Integer32 | WasiParamKind::Char => ListElement::Word,
        WasiParamKind::IntegerNarrow { bits, signed } => ListElement::Narrow {
            bits: *bits,
            signed: *signed,
        },
        WasiParamKind::Boolean => ListElement::Boolean,
        WasiParamKind::Scalar64 { signed } => ListElement::Scalar64 { signed: *signed },
        WasiParamKind::Float32 => ListElement::Float32,
        WasiParamKind::Float64 => ListElement::Float64,
        WasiParamKind::List => ListElement::String,
        WasiParamKind::ValueList { .. }
        | WasiParamKind::Enum { .. }
        | WasiParamKind::Flags { .. }
        | WasiParamKind::Handle(_)
        | WasiParamKind::Record { .. }
        | WasiParamKind::Unsupported => return None,
    })
}

/// Canonical `(size, align)` of one element, in bytes.
pub(crate) fn element_layout(element: ListElement) -> (i32, i32) {
    match element {
        ListElement::Word | ListElement::Float32 => (4, 4),
        ListElement::Narrow { bits: 8, .. } | ListElement::Boolean => (1, 1),
        ListElement::Narrow { bits: 16, .. } => (2, 2),
        ListElement::Narrow { .. } => (4, 4),
        ListElement::Scalar64 { .. } | ListElement::Float64 => (8, 8),
        ListElement::String => (8, 4),
    }
}
