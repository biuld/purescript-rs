//! Canonical size and alignment of a canonical type on the wasm32 target.
//!
//! This mirrors `wit_parser`'s `SizeAlign` for a 32-bit pointer: a pointer is
//! four bytes, so a string or list is `(pointer, length)` with size eight and
//! alignment four. It drives the return area, indirect parameter records, and
//! canonical memory stores.

use super::{CanonicalType, discriminant_size};

/// The canonical size and alignment in bytes of one value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SizeAlign {
    pub size: u32,
    pub align: u32,
}

fn size_align_of(size: u32, align: u32) -> SizeAlign {
    SizeAlign { size, align }
}

/// The canonical size and alignment of `ty`.
pub(crate) fn size_align(ty: &CanonicalType) -> SizeAlign {
    match ty {
        CanonicalType::Bool => size_align_of(1, 1),
        CanonicalType::Int { width: 8, .. } => size_align_of(1, 1),
        CanonicalType::Int { width: 16, .. } => size_align_of(2, 2),
        CanonicalType::Int { width: 64, .. } => size_align_of(8, 8),
        CanonicalType::Int { .. } => size_align_of(4, 4),
        CanonicalType::Float { width: 32 } => size_align_of(4, 4),
        CanonicalType::Float { .. } => size_align_of(8, 8),
        CanonicalType::Char => size_align_of(4, 4),
        CanonicalType::Handle { .. } => size_align_of(4, 4),
        CanonicalType::String | CanonicalType::List(_) => size_align_of(8, 4),
        CanonicalType::FixedList { element, length } => {
            let element = size_align(element);
            size_align_of(element.size * length, element.align)
        }
        CanonicalType::Record(fields) => record(fields.iter().map(|field| &field.ty)),
        CanonicalType::Flags(names) => flags(names.len()),
        CanonicalType::Enum(cases) => variant(discriminant_size(cases.len()), []),
        CanonicalType::Option(payload) => variant(1, std::iter::once(Some(payload.as_ref()))),
        CanonicalType::Result { ok, err } => variant(1, [ok.as_deref(), err.as_deref()]),
        CanonicalType::Variant(cases) => variant(
            discriminant_size(cases.len()),
            cases.iter().map(|case| case.payload.as_deref()),
        ),
    }
}

/// A record lays its fields at their aligned offsets and rounds its size up to
/// the record alignment.
fn record<'a>(fields: impl IntoIterator<Item = &'a CanonicalType>) -> SizeAlign {
    let mut size = 0u32;
    let mut align = 1u32;
    for field in fields {
        let field = size_align(field);
        size = align_up(size, field.align) + field.size;
        align = align.max(field.align);
    }
    size_align_of(align_up(size, align), align)
}

/// A tagged type is a discriminant followed by the largest case payload, with
/// the payload aligned to the largest case alignment.
fn variant<'a>(
    discriminant: u32,
    cases: impl IntoIterator<Item = Option<&'a CanonicalType>>,
) -> SizeAlign {
    let mut payload_size = 0u32;
    let mut payload_align = 1u32;
    for case in cases.into_iter().flatten() {
        let case = size_align(case);
        payload_size = payload_size.max(case.size);
        payload_align = payload_align.max(case.align);
    }
    let align = discriminant.max(payload_align);
    let offset = align_up(discriminant, payload_align);
    size_align_of(align_up(offset + payload_size, align), align)
}

/// Flags pack into a `u8`/`u16` or a whole number of `i32` words.
fn flags(names: usize) -> SizeAlign {
    match names {
        0 => size_align_of(0, 4),
        1..=8 => size_align_of(1, 1),
        9..=16 => size_align_of(2, 2),
        _ => size_align_of(4 * names.div_ceil(32) as u32, 4),
    }
}

/// Rounds `value` up to the next multiple of `align`, a power of two.
fn align_up(value: u32, align: u32) -> u32 {
    value.next_multiple_of(align)
}
