//! The normalized, recursive canonical ABI type and its generic operations.
//!
//! This is the single description of a WIT value the ABI layer consults: a
//! [`CanonicalType`] resolved once from `wit_parser`, with `flatten`,
//! `size_align`, and `despecialize` as recursive operations over it. Because
//! classification and flattening read the same value, they cannot diverge the
//! way the parallel `WasiParamKind` / `FlatSlot` descriptors could. See
//! `docs/design/backend/wasm/canonical-abi-compositional.md`.
//!
//! Migration step 1 introduces this model additively. The existing descriptor
//! path still drives lowering, so the module is not yet consumed by production
//! code; `dead_code` is allowed until the migration reaches its callers.
#![allow(dead_code)]

mod flatten;
mod memory;
mod plan;
mod resolve;

#[cfg(test)]
mod tests;

pub(crate) use flatten::flatten;
// `size_align` and `function_abi` are exercised by tests now and consumed by
// lowering in migration step 2; allow the staged re-exports in the lib target.
#[allow(unused_imports)]
pub(crate) use memory::{SizeAlign, size_align};
#[allow(unused_imports)]
pub(crate) use plan::function_abi;
pub(crate) use resolve::resolve;

/// A core WebAssembly value type a canonical type flattens to on the wasm32
/// target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoreVal {
    I32,
    I64,
    F32,
    F64,
}

/// The normalized canonical ABI type of one WIT value.
///
/// Aliases are followed and tuples are labeled records; `option`, `result`,
/// `enum`, and `variant` stay distinct until [`despecialize`] folds them, so the
/// source mapping and diagnostics keep the WIT form. `String` is a byte list on
/// the wire but a GC UTF-16 value in the guest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CanonicalType {
    Bool,
    Int {
        width: u8,
        signed: bool,
    },
    Float {
        width: u8,
    },
    Char,
    /// `string`; a byte sequence on the wire.
    String,
    List(Box<CanonicalType>),
    FixedList {
        element: Box<CanonicalType>,
        length: u32,
    },
    Record(Vec<CanonicalField>),
    Variant(Vec<CanonicalCase>),
    Option(Box<CanonicalType>),
    Result {
        ok: Option<Box<CanonicalType>>,
        err: Option<Box<CanonicalType>>,
    },
    Enum(Vec<String>),
    Flags(Vec<String>),
    Handle {
        resource: ResourceId,
        ownership: Ownership,
    },
}

impl CanonicalType {
    fn int(width: u8, signed: bool) -> Self {
        CanonicalType::Int { width, signed }
    }
}

/// One labeled field of a canonical record, in WIT declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CanonicalField {
    pub name: String,
    pub ty: CanonicalType,
}

/// One case of a canonical variant. `payload` is `None` for a nullary case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CanonicalCase {
    pub name: String,
    pub payload: Option<Box<CanonicalType>>,
}

/// The WIT resource a handle refers to, named by its canonical interface id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResourceId {
    pub interface: String,
    pub name: String,
}

/// Whether a handle owns the resource or only borrows it for one call.
///
/// Ownership is wire metadata on a handle; the drop symbol and interface are
/// bound to the import by the ABI side table, not carried here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ownership {
    Own,
    Borrow,
}

/// Rewrites `option`, `result`, and `enum` into an equivalent `variant` so a
/// consumer can treat every tagged type uniformly. It recurses through fields,
/// elements, and payloads, and is idempotent: a `variant` is returned unchanged.
pub(crate) fn despecialize(ty: &CanonicalType) -> CanonicalType {
    match ty {
        CanonicalType::Option(payload) => CanonicalType::Variant(vec![
            CanonicalCase {
                name: "none".into(),
                payload: None,
            },
            CanonicalCase {
                name: "some".into(),
                payload: Some(Box::new(despecialize(payload))),
            },
        ]),
        CanonicalType::Result { ok, err } => CanonicalType::Variant(vec![
            CanonicalCase {
                name: "ok".into(),
                payload: ok.as_deref().map(|ty| Box::new(despecialize(ty))),
            },
            CanonicalCase {
                name: "err".into(),
                payload: err.as_deref().map(|ty| Box::new(despecialize(ty))),
            },
        ]),
        CanonicalType::Enum(cases) => CanonicalType::Variant(
            cases
                .iter()
                .map(|name| CanonicalCase {
                    name: name.clone(),
                    payload: None,
                })
                .collect(),
        ),
        CanonicalType::List(element) => CanonicalType::List(Box::new(despecialize(element))),
        CanonicalType::FixedList { element, length } => CanonicalType::FixedList {
            element: Box::new(despecialize(element)),
            length: *length,
        },
        CanonicalType::Record(fields) => CanonicalType::Record(
            fields
                .iter()
                .map(|field| CanonicalField {
                    name: field.name.clone(),
                    ty: despecialize(&field.ty),
                })
                .collect(),
        ),
        CanonicalType::Variant(cases) => CanonicalType::Variant(
            cases
                .iter()
                .map(|case| CanonicalCase {
                    name: case.name.clone(),
                    payload: case.payload.as_deref().map(|ty| Box::new(despecialize(ty))),
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The width in bytes of a tagged type's discriminant, from `wit_parser`'s
/// `discriminant_type`: one byte up to 256 cases, two up to 65536, else four.
fn discriminant_size(cases: usize) -> u32 {
    match cases.checked_sub(1) {
        None => 1,
        Some(tag) if tag <= u8::MAX as usize => 1,
        Some(tag) if tag <= u16::MAX as usize => 2,
        Some(_) => 4,
    }
}
