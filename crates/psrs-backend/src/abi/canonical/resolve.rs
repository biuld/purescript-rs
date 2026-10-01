//! Resolves a `wit_parser` type into the normalized canonical type.
//!
//! This is the only ABI code that reads `wit_parser`'s type structure. It
//! follows aliases, labels tuples `_1`.., and records handle ownership. A shape
//! with no canonical representation (`map`, `future`, `stream`, a raw resource,
//! `error-context`, or an unknown definition) resolves to `None` so the caller
//! rejects it rather than approximating it.

use super::{CanonicalCase, CanonicalField, CanonicalType, Ownership, ResourceId};
use crate::abi::handles::UNBOUND_DROP;
use wit_parser::{Case, Field, Handle, Resolve, Type as WitType, TypeDefKind, TypeId};

/// Resolves a WIT type, or `None` when it has no source-level canonical form.
pub(crate) fn resolve(resolve: &Resolve, ty: &WitType) -> Option<CanonicalType> {
    Some(match ty {
        WitType::Bool => CanonicalType::Bool,
        WitType::S8 => CanonicalType::int(8, true),
        WitType::U8 => CanonicalType::int(8, false),
        WitType::S16 => CanonicalType::int(16, true),
        WitType::U16 => CanonicalType::int(16, false),
        WitType::S32 => CanonicalType::int(32, true),
        WitType::U32 => CanonicalType::int(32, false),
        WitType::S64 => CanonicalType::int(64, true),
        WitType::U64 => CanonicalType::int(64, false),
        WitType::F32 => CanonicalType::Float { width: 32 },
        WitType::F64 => CanonicalType::Float { width: 64 },
        WitType::Char => CanonicalType::Char,
        WitType::String => CanonicalType::String,
        WitType::ErrorContext => return None,
        WitType::Id(id) => return resolve_id(resolve, *id),
    })
}

fn resolve_id(wit: &Resolve, id: TypeId) -> Option<CanonicalType> {
    Some(match &wit.types[id].kind {
        TypeDefKind::Type(inner) => return resolve(wit, inner),
        TypeDefKind::List(inner) => CanonicalType::List(Box::new(resolve(wit, inner)?)),
        TypeDefKind::FixedLengthList(inner, length) => CanonicalType::FixedList {
            element: Box::new(resolve(wit, inner)?),
            length: *length,
        },
        TypeDefKind::Record(record) => CanonicalType::Record(
            record
                .fields
                .iter()
                .map(|field| resolve_field(wit, field))
                .collect::<Option<Vec<_>>>()?,
        ),
        TypeDefKind::Tuple(tuple) => CanonicalType::Record(
            tuple
                .types
                .iter()
                .enumerate()
                .map(|(index, ty)| {
                    Some(CanonicalField {
                        name: format!("_{}", index + 1),
                        ty: resolve(wit, ty)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        TypeDefKind::Variant(variant) => CanonicalType::Variant(
            variant
                .cases
                .iter()
                .map(|case| resolve_case(wit, case))
                .collect::<Option<Vec<_>>>()?,
        ),
        TypeDefKind::Option(inner) => CanonicalType::Option(Box::new(resolve(wit, inner)?)),
        TypeDefKind::Result(result) => CanonicalType::Result {
            ok: resolve_optional(wit, result.ok.as_ref())?,
            err: resolve_optional(wit, result.err.as_ref())?,
        },
        TypeDefKind::Enum(enum_) => {
            CanonicalType::Enum(enum_.cases.iter().map(|case| case.name.clone()).collect())
        }
        TypeDefKind::Flags(flags) => {
            CanonicalType::Flags(flags.flags.iter().map(|flag| flag.name.clone()).collect())
        }
        TypeDefKind::Handle(handle) => return resolve_handle(wit, handle),
        TypeDefKind::Map(..)
        | TypeDefKind::Future(_)
        | TypeDefKind::Stream(_)
        | TypeDefKind::Resource
        | TypeDefKind::Unknown => return None,
    })
}

fn resolve_field(wit: &Resolve, field: &Field) -> Option<CanonicalField> {
    Some(CanonicalField {
        name: field.name.clone(),
        ty: resolve(wit, &field.ty)?,
    })
}

fn resolve_case(wit: &Resolve, case: &Case) -> Option<CanonicalCase> {
    Some(CanonicalCase {
        name: case.name.clone(),
        payload: resolve_optional(wit, case.ty.as_ref())?,
    })
}

fn resolve_optional(wit: &Resolve, ty: Option<&WitType>) -> Option<Option<Box<CanonicalType>>> {
    match ty {
        None => Some(None),
        Some(ty) => Some(Some(Box::new(resolve(wit, ty)?))),
    }
}

fn resolve_handle(wit: &Resolve, handle: &Handle) -> Option<CanonicalType> {
    let handle = super::super::handles::classify(wit, handle)?;
    let ownership = match handle.mode {
        super::super::HandleMode::Own => Ownership::Own { drop: UNBOUND_DROP },
        super::super::HandleMode::Borrow => Ownership::Borrow,
    };
    Some(CanonicalType::Handle {
        resource: ResourceId {
            interface: handle.interface,
            name: handle.name,
        },
        ownership,
    })
}
