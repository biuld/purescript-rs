//! Flattening a canonical type to core WebAssembly values.
//!
//! This mirrors `wit_parser`'s `push_flat`/`join` on the wasm32 target: a variant
//! joins its case payloads position-wise and extends to the longest case, so
//! flattening is total. The indirect-parameter and return-pointer decisions are
//! the parameter and result length thresholds in [`super::plan`], not a property
//! of the type.

use super::{CanonicalType, CoreVal};
use CoreVal::{F32, F64, I32, I64};

/// The core values `ty` flattens to, in order.
pub(crate) fn flatten(ty: &CanonicalType) -> Vec<CoreVal> {
    let mut flat = Vec::new();
    push(ty, &mut flat);
    flat
}

fn push(ty: &CanonicalType, flat: &mut Vec<CoreVal>) {
    match ty {
        CanonicalType::Bool | CanonicalType::Char | CanonicalType::Enum(_) => flat.push(I32),
        CanonicalType::Int { width: 64, .. } => flat.push(I64),
        CanonicalType::Int { .. } => flat.push(I32),
        CanonicalType::Float { width: 64 } => flat.push(F64),
        CanonicalType::Float { .. } => flat.push(F32),
        CanonicalType::Handle { .. } => flat.push(I32),
        CanonicalType::String | CanonicalType::List(_) => {
            flat.push(I32);
            flat.push(I32);
        }
        CanonicalType::FixedList { element, length } => {
            for _ in 0..*length {
                push(element, flat);
            }
        }
        CanonicalType::Record(fields) => {
            for field in fields {
                push(&field.ty, flat);
            }
        }
        CanonicalType::Flags(names) => {
            for _ in 0..flag_words(names.len()) {
                flat.push(I32);
            }
        }
        CanonicalType::Option(payload) => {
            flat.push(I32);
            push(payload, flat);
        }
        CanonicalType::Result { ok, err } => {
            flat.push(I32);
            push_joined([ok.as_deref(), err.as_deref()], flat);
        }
        CanonicalType::Variant(cases) => {
            flat.push(I32);
            push_joined(cases.iter().map(|case| case.payload.as_deref()), flat);
        }
    }
}

/// Joins the flattened payloads of a tagged type's cases: the result has the
/// longest case length, and each position is the [`join`] of the cases that
/// reach it. Nullary cases contribute nothing.
fn push_joined<'a>(
    cases: impl IntoIterator<Item = Option<&'a CanonicalType>>,
    flat: &mut Vec<CoreVal>,
) {
    let cases = cases.into_iter().flatten().map(flatten).collect::<Vec<_>>();
    let length = cases.iter().map(Vec::len).max().unwrap_or(0);
    for index in 0..length {
        let mut slot: Option<CoreVal> = None;
        for flat_case in &cases {
            if let Some(value) = flat_case.get(index).copied() {
                slot = Some(match slot {
                    None => value,
                    Some(previous) => join(previous, value),
                });
            }
        }
        flat.push(slot.expect("every position below the maximum length is covered"));
    }
}

/// Unifies two flattened slot types, matching `wit_parser::abi::join` collapsed
/// to the actual wasm32 value types: an integer/float pair at 32 bits becomes
/// `i32`, and any pair involving a 64-bit value becomes `i64`.
fn join(left: CoreVal, right: CoreVal) -> CoreVal {
    if left == right {
        return left;
    }
    match (left, right) {
        (I32, F32) | (F32, I32) => I32,
        _ => I64,
    }
}

/// The number of `i32` words a `flags` value with `names` flags flattens to:
/// zero for an empty flags, else `ceil(names / 32)`.
fn flag_words(names: usize) -> usize {
    if names == 0 { 0 } else { names.div_ceil(32) }
}
