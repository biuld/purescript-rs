//! The normalized, recursive canonical ABI type and its generic operations.
//!
//! This is the single description of a WIT value the ABI layer consults: a
//! [`CanonicalType`] resolved once from `wit_parser`, with `flatten`,
//! `size_align`, and `despecialize` as recursive operations over it. Because
//! every consumer reads the same value, classification and lowering cannot
//! diverge. See `docs/design/backend/wasm/canonical-abi-compositional.md`.
//!
//! This model is the only ABI vocabulary: classification, flattening, layout,
//! conformance, and MIR lowering all read a [`CanonicalType`] directly.
//! `dead_code` is not allowed; every operation has a caller.

mod flatten;
mod leaves;
mod memory;
mod plan;
mod resolve;

#[cfg(test)]
mod tests;

pub(crate) use flatten::flatten;
pub(crate) use leaves::{FlatLeaf, flat_leaves_of, handle_at_flat_index};
#[allow(unused_imports)]
pub(crate) use memory::SizeAlign;
pub(crate) use memory::size_align;
pub(crate) use plan::{FnAbi, function_abi, function_abi_from_types};
pub(crate) use resolve::resolve;

use psrs_hir::SymbolId;

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
pub enum CanonicalType {
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

    /// Whether this is the byte element `u8`.
    pub(crate) fn is_byte(&self) -> bool {
        matches!(
            self,
            CanonicalType::Int {
                width: 8,
                signed: false,
            }
        )
    }

    /// Whether this is `string` or a list of bytes, which share the
    /// `(pointer, length)` canonical representation.
    pub(crate) fn is_byte_list(&self) -> bool {
        match self {
            CanonicalType::String => true,
            CanonicalType::List(inner) | CanonicalType::FixedList { element: inner, .. } => {
                inner.is_byte()
            }
            _ => false,
        }
    }
}

/// One labeled field of a canonical record, in WIT declaration order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalField {
    pub name: String,
    pub ty: CanonicalType,
}

/// One case of a canonical variant. `payload` is `None` for a nullary case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalCase {
    pub name: String,
    pub payload: Option<Box<CanonicalType>>,
}

/// The WIT resource a handle refers to, named by its canonical interface id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceId {
    pub interface: String,
    pub name: String,
}

/// Whether a handle owns the resource or only borrows it for one call.
///
/// An owned handle carries the `[resource-drop]<T>` symbol that releases it.
/// The symbol starts as [`super::handles::UNBOUND_DROP`] and is bound when the
/// import that mentions the handle is interned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ownership {
    Own { drop: SymbolId },
    Borrow,
}

impl Ownership {
    /// The drop symbol for an owned handle; a borrow has none.
    pub(crate) fn drop_symbol(&self) -> Option<SymbolId> {
        match self {
            Ownership::Own { drop } => Some(*drop),
            Ownership::Borrow => None,
        }
    }
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

/// The source constructor name for a WIT case name: split on `-`, uppercase the
/// first letter of each part, and join with no separator.
pub(crate) fn source_constructor_name(wit_case: &str) -> String {
    wit_case
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            let mut name = String::new();
            if let Some(first) = chars.next() {
                name.extend(first.to_uppercase());
                name.extend(chars);
            }
            name
        })
        .collect()
}

/// Whether a tagged type (`option`, `result`, or `variant`) can be treated as a
/// variant with tag-ordered payloads.
pub(crate) fn is_variant(ty: &CanonicalType) -> bool {
    matches!(
        ty,
        CanonicalType::Option(_) | CanonicalType::Result { .. } | CanonicalType::Variant(_)
    )
}

/// The tag-ordered payloads of a tagged type. `None` when `ty` is not a variant.
pub(crate) fn payload_cases(ty: &CanonicalType) -> Option<Vec<Option<&CanonicalType>>> {
    Some(match ty {
        CanonicalType::Option(payload) => vec![None, Some(payload)],
        CanonicalType::Result { ok, err } => vec![ok.as_deref(), err.as_deref()],
        CanonicalType::Variant(cases) => cases.iter().map(|case| case.payload.as_deref()).collect(),
        _ => return None,
    })
}

/// The guest constructor tag of the source case that canonical case `index`
/// maps to.
///
/// `option`, `variant`, and `enum` cases correspond to source constructors in
/// tag order, so the mapping is the identity. A canonical `result` orders its
/// cases `[ok, err]`, while the source `Either` constructors are `Left` (the
/// error, tag 0) then `Right` (the ok value, tag 1) ([DEC-13]); the two tags
/// swap. Every `result` has exactly two cases.
pub(crate) fn source_tag_for_case(ty: &CanonicalType, index: usize) -> usize {
    match ty {
        CanonicalType::Result { .. } => 1 - index,
        _ => index,
    }
}

/// The canonical case index that a guest `source_tag` selects. The inverse of
/// [`source_tag_for_case`].
pub(crate) fn canonical_case_for_tag(ty: &CanonicalType, source_tag: usize) -> usize {
    match ty {
        CanonicalType::Result { .. } => 1 - source_tag,
        _ => source_tag,
    }
}

/// Whether a tagged type swaps canonical and source case tags. Only `result`
/// does; `option`, `variant`, and `enum` keep tag order.
pub(crate) fn swaps_case_tags(ty: &CanonicalType) -> bool {
    matches!(ty, CanonicalType::Result { .. })
}

/// A top-level `list<T>` of a supported element is lowered. A non-byte `list`
/// nested directly in a record field is still rejected, but a list nested in a
/// `list`, `option`, `result`, or `variant` is lowered recursively.
pub(crate) fn contains_rejected_list(ty: &CanonicalType, nested: bool) -> bool {
    match ty {
        CanonicalType::List(inner) => {
            if nested {
                !inner.is_byte()
            } else {
                !supported_list_element(inner)
            }
        }
        CanonicalType::FixedList { element, .. } => !supported_list_element(element),
        CanonicalType::Record(fields) => fields
            .iter()
            .any(|field| contains_rejected_list(&field.ty, true)),
        CanonicalType::Option(inner) => contains_rejected_list(inner, nested),
        CanonicalType::Result { ok, err } => {
            ok.as_deref()
                .is_some_and(|ty| contains_rejected_list(ty, nested))
                || err
                    .as_deref()
                    .is_some_and(|ty| contains_rejected_list(ty, nested))
        }
        CanonicalType::Variant(cases) => cases.iter().any(|case| {
            case.payload
                .as_deref()
                .is_some_and(|ty| contains_rejected_list(ty, nested))
        }),
        _ => false,
    }
}

/// Whether a value can be a list element, a fixed-length list element, a
/// variant payload, or a record field inside a copied list. Every aggregate
/// recurses, so `list<option<string>>`, `list<list<T>>`, and a record with a
/// nested list are admitted.
pub(crate) fn supported_list_element(ty: &CanonicalType) -> bool {
    match ty {
        CanonicalType::String
        | CanonicalType::Bool
        | CanonicalType::Int { .. }
        | CanonicalType::Float { .. }
        | CanonicalType::Char
        | CanonicalType::Enum(_)
        | CanonicalType::Flags(_)
        | CanonicalType::Handle { .. } => true,
        CanonicalType::List(inner) => supported_list_element(inner),
        CanonicalType::FixedList { element, .. } => supported_list_element(element),
        CanonicalType::Record(fields) => {
            fields.iter().all(|field| supported_list_element(&field.ty))
        }
        CanonicalType::Option(inner) => supported_list_element(inner),
        CanonicalType::Result { ok, err } => {
            ok.as_deref().is_none_or(supported_list_element)
                && err.as_deref().is_none_or(supported_list_element)
        }
        CanonicalType::Variant(cases) => cases
            .iter()
            .all(|case| case.payload.as_deref().is_none_or(supported_list_element)),
    }
}

/// Whether a type can be a directly lowered payload of a record, option, result,
/// or variant.
fn direct_parameter(ty: &CanonicalType) -> bool {
    match ty {
        CanonicalType::Bool
        | CanonicalType::Int { .. }
        | CanonicalType::Float { .. }
        | CanonicalType::Char
        | CanonicalType::String
        | CanonicalType::Enum(_)
        | CanonicalType::Flags(_)
        | CanonicalType::Handle { .. } => true,
        CanonicalType::List(_) => true,
        CanonicalType::FixedList { element, .. } => direct_parameter(element),
        CanonicalType::Record(fields) => fields.iter().all(|field| direct_parameter(&field.ty)),
        CanonicalType::Option(inner) => direct_parameter(inner),
        CanonicalType::Result { ok, err } => {
            ok.as_deref().is_none_or(direct_parameter)
                && err.as_deref().is_none_or(direct_parameter)
        }
        CanonicalType::Variant(cases) => cases
            .iter()
            .all(|case| case.payload.as_deref().is_none_or(direct_parameter)),
    }
}

/// Whether an otherwise unsupported canonical type is only scalars, handles,
/// byte lists, and `option` / `result` / tuple / variant structure around them.
pub(crate) fn primitive_aggregate_allowed(ty: &CanonicalType) -> bool {
    match ty {
        CanonicalType::Bool
        | CanonicalType::Int { .. }
        | CanonicalType::Float { .. }
        | CanonicalType::Char
        | CanonicalType::String
        | CanonicalType::Handle { .. }
        | CanonicalType::Enum(_)
        | CanonicalType::Flags(_) => true,
        CanonicalType::List(element) | CanonicalType::FixedList { element, .. } => {
            element.is_byte()
        }
        CanonicalType::Record(fields) => fields
            .iter()
            .all(|field| primitive_aggregate_allowed(&field.ty)),
        CanonicalType::Option(payload) => primitive_aggregate_allowed(payload),
        CanonicalType::Result { ok, err } => {
            ok.as_deref().is_none_or(primitive_aggregate_allowed)
                && err.as_deref().is_none_or(primitive_aggregate_allowed)
        }
        CanonicalType::Variant(cases) => cases.iter().all(|case| {
            case.payload
                .as_deref()
                .is_none_or(primitive_aggregate_allowed)
        }),
    }
}

/// Whether a canonical type has a source-ABI parameter lowering. This mirrors
/// the shapes the MIR lowering models directly; a false result combined with
/// [`primitive_aggregate_allowed`] defers the shape to the primitive-FFI check.
pub(crate) fn parameter_has_source_abi(ty: &CanonicalType) -> bool {
    match ty {
        CanonicalType::Bool
        | CanonicalType::Int { .. }
        | CanonicalType::Float { .. }
        | CanonicalType::Char
        | CanonicalType::String
        | CanonicalType::Enum(_)
        | CanonicalType::Flags(_)
        | CanonicalType::Handle { .. }
        | CanonicalType::List(_) => true,
        CanonicalType::FixedList { element, .. } => supported_list_element(element),
        CanonicalType::Record(fields) => fields.iter().all(|field| direct_parameter(&field.ty)),
        CanonicalType::Option(inner) => direct_parameter(inner),
        CanonicalType::Result { ok, err } => {
            ok.as_deref().is_none_or(direct_parameter)
                && err.as_deref().is_none_or(direct_parameter)
        }
        CanonicalType::Variant(cases) => cases
            .iter()
            .all(|case| case.payload.as_deref().is_none_or(direct_parameter)),
    }
}
