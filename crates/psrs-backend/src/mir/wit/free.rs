//! The recursive free plan for canonical call-local buffers.
//!
//! A canonical value owns the transient buffers the canonical ABI allocated for
//! it: a `string`/non-byte-list buffer, and, inside a list buffer, each
//! element's own buffers. [`FreePlan`] is the compile-time shape of those
//! buffers, derived from the canonical type and its guest layout. The wasm
//! structurer frees exactly the buffers `lower` allocated, selected by the same
//! tag `lift` read, so no buffer is freed twice and none leaks.

use crate::abi::canonical::CanonicalType;

/// A recursive description of the transient buffers a canonical value owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FreePlan {
    /// Nothing to free: a scalar, handle, flags, or byte element.
    NoFree,
    /// A `(pointer, length)` buffer, such as a string transcode buffer.
    Buffer { align: u32 },
    /// A product of fields, each with its own plan.
    Fields(Vec<FreePlan>),
    /// A tagged value; the plan of the selected case is freed.
    Case(Vec<FreePlan>),
    /// A list buffer: free each element's plan, then the buffer itself.
    Elements(Box<FreePlan>),
}

impl FreePlan {
    /// Whether this plan frees anything.
    pub(super) fn is_no_free(&self) -> bool {
        match self {
            FreePlan::NoFree => true,
            FreePlan::Buffer { .. } | FreePlan::Elements(_) => false,
            FreePlan::Fields(fields) => fields.iter().all(FreePlan::is_no_free),
            FreePlan::Case(cases) => cases.iter().all(FreePlan::is_no_free),
        }
    }
}

/// Derives the free plan of a canonical type. The guest layout is not consulted
/// because the free buffers are exactly the `(pointer, length)` pairs the
/// canonical type lays out.
pub(super) fn free_plan(ty: &CanonicalType) -> FreePlan {
    match ty {
        CanonicalType::String => FreePlan::Buffer { align: 1 },
        CanonicalType::List(element) | CanonicalType::FixedList { element, .. } => {
            if element.is_byte() {
                FreePlan::Elements(Box::new(FreePlan::NoFree))
            } else {
                FreePlan::Elements(Box::new(free_plan(element)))
            }
        }
        CanonicalType::Record(fields) => {
            FreePlan::Fields(fields.iter().map(|field| free_plan(&field.ty)).collect())
        }
        CanonicalType::Option(payload) => {
            FreePlan::Case(vec![FreePlan::NoFree, free_plan(payload)])
        }
        CanonicalType::Result { ok, err } => FreePlan::Case(vec![
            ok.as_deref().map_or(FreePlan::NoFree, free_plan),
            err.as_deref().map_or(FreePlan::NoFree, free_plan),
        ]),
        CanonicalType::Variant(cases) => FreePlan::Case(
            cases
                .iter()
                .map(|case| case.payload.as_deref().map_or(FreePlan::NoFree, free_plan))
                .collect(),
        ),
        CanonicalType::Bool
        | CanonicalType::Int { .. }
        | CanonicalType::Float { .. }
        | CanonicalType::Char
        | CanonicalType::Enum(_)
        | CanonicalType::Flags(_)
        | CanonicalType::Handle { .. } => FreePlan::NoFree,
    }
}
