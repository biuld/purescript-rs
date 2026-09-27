//! Flat source leaves of a canonical type.
//!
//! A variable-length list, a tagged type, and a record all produce several core
//! values. A primitive foreign import names those values directly (`Int` for a
//! discriminant, handle, or narrow integer, `String` for `(pointer, length)`)
//! instead of a compiler aggregate. [`FlatLeaf`] records which source primitive
//! may fill a position, so the primitive-FFI path can map a declaration onto the
//! canonical flat values. It is derived from the canonical type, never stored.

use super::CanonicalType;
use crate::types::ValueType;

/// One core slot in a canonical flattening, excluding a return pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlatLeaf {
    Int32,
    Int64 {
        signed: bool,
    },
    Boolean,
    Char,
    Float32,
    Float64,
    Handle,
    Pointer,
    Length,
    /// Joined variant payloads that are not one source primitive.
    Ambiguous,
}

impl FlatLeaf {
    /// The core value type a leaf occupies, or `None` for an ambiguous slot.
    pub(crate) fn value_type(self) -> Option<ValueType> {
        Some(match self {
            FlatLeaf::Int32
            | FlatLeaf::Boolean
            | FlatLeaf::Char
            | FlatLeaf::Handle
            | FlatLeaf::Pointer
            | FlatLeaf::Length => ValueType::I32,
            FlatLeaf::Int64 { .. } => ValueType::I64,
            FlatLeaf::Float32 => ValueType::F32,
            FlatLeaf::Float64 => ValueType::F64,
            FlatLeaf::Ambiguous => return None,
        })
    }
}

/// The flat leaves every parameter produces, in order.
pub(crate) fn flat_leaves_of<'a>(
    params: impl IntoIterator<Item = &'a CanonicalType>,
) -> Vec<FlatLeaf> {
    let mut leaves = Vec::new();
    for ty in params {
        push(ty, &mut leaves);
    }
    leaves
}

fn push(ty: &CanonicalType, leaves: &mut Vec<FlatLeaf>) {
    match ty {
        CanonicalType::Bool => leaves.push(FlatLeaf::Boolean),
        CanonicalType::Int { width: 64, signed } => {
            leaves.push(FlatLeaf::Int64 { signed: *signed })
        }
        CanonicalType::Int { .. } => leaves.push(FlatLeaf::Int32),
        CanonicalType::Float { width: 32 } => leaves.push(FlatLeaf::Float32),
        CanonicalType::Float { .. } => leaves.push(FlatLeaf::Float64),
        CanonicalType::Char => leaves.push(FlatLeaf::Char),
        CanonicalType::String | CanonicalType::List(_) => {
            leaves.push(FlatLeaf::Pointer);
            leaves.push(FlatLeaf::Length);
        }
        CanonicalType::FixedList { element, length } => {
            for _ in 0..*length {
                push(element, leaves);
            }
        }
        CanonicalType::Record(fields) => {
            for field in fields {
                push(&field.ty, leaves);
            }
        }
        CanonicalType::Flags(names) => {
            for _ in 0..flag_words(names.len()) {
                leaves.push(FlatLeaf::Int32);
            }
        }
        CanonicalType::Enum(_) => leaves.push(FlatLeaf::Int32),
        CanonicalType::Handle { .. } => leaves.push(FlatLeaf::Handle),
        CanonicalType::Option(payload) => {
            leaves.push(FlatLeaf::Int32);
            push_cases([None, Some(payload.as_ref())], leaves);
        }
        CanonicalType::Result { ok, err } => {
            leaves.push(FlatLeaf::Int32);
            push_cases([ok.as_deref(), err.as_deref()], leaves);
        }
        CanonicalType::Variant(cases) => {
            leaves.push(FlatLeaf::Int32);
            push_cases(cases.iter().map(|case| case.payload.as_deref()), leaves);
        }
    }
}

/// The number of `i32` words a `flags` value with `names` flags flattens to:
/// zero for an empty flags, else `ceil(names / 32)`.
fn flag_words(names: usize) -> usize {
    if names == 0 { 0 } else { names.div_ceil(32) }
}

/// Merges each case's payload the way the canonical ABI joins variant payloads.
/// An empty case contributes no slots. Distinct source leaves in the same slot
/// become [`FlatLeaf::Ambiguous`] so a primitive import is rejected.
fn push_cases<'a>(
    cases: impl IntoIterator<Item = Option<&'a CanonicalType>>,
    leaves: &mut Vec<FlatLeaf>,
) {
    let mut merged = Vec::new();
    for case in cases {
        let Some(ty) = case else {
            continue;
        };
        let mut payload = Vec::new();
        push(ty, &mut payload);
        for (index, leaf) in payload.into_iter().enumerate() {
            if let Some(existing) = merged.get_mut(index) {
                if *existing != leaf {
                    *existing = FlatLeaf::Ambiguous;
                }
            } else {
                merged.push(leaf);
            }
        }
    }
    leaves.extend(merged);
}

/// The canonical handle whose flat slot is `flat_index`, if one covers it.
///
/// A record recurses into its fields; an `option` skips its discriminant; any
/// other shape advances the cursor by its flattened width.
pub(crate) fn handle_at_flat_index<'a>(
    params: impl IntoIterator<Item = &'a CanonicalType>,
    flat_index: usize,
) -> Option<&'a CanonicalType> {
    let mut cursor = 0;
    for ty in params {
        if let Some(found) = cover(ty, flat_index, &mut cursor) {
            return Some(found);
        }
    }
    None
}

fn cover<'a>(
    ty: &'a CanonicalType,
    target: usize,
    cursor: &mut usize,
) -> Option<&'a CanonicalType> {
    match ty {
        CanonicalType::Handle { .. } => {
            let here = *cursor;
            *cursor += 1;
            (here == target).then_some(ty)
        }
        CanonicalType::Record(fields) => {
            for field in fields {
                if let Some(found) = cover(&field.ty, target, cursor) {
                    return Some(found);
                }
            }
            None
        }
        CanonicalType::Option(payload) => {
            *cursor += 1;
            cover(payload, target, cursor)
        }
        other => {
            *cursor += super::flatten::flatten(other).len();
            None
        }
    }
}
