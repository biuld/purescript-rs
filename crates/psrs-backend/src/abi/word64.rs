//! Exact source carrier for a canonical 64-bit integer bit pattern.

use psrs_core::{Module, Type, TypeConstructor, TypeId};

pub(crate) const LABELS: [&str; 2] = ["high", "low"];

pub(crate) fn is_source(module: &Module, id: TypeId) -> bool {
    if module.record_is_open(id) != Some(false) {
        return false;
    }
    let Some(fields) = module.record_fields(id) else {
        return false;
    };
    fields.len() == 2
        && LABELS.iter().zip(&fields).all(|(label, (name, ty))| {
            *label == name.as_str()
                && module.types.get(ty.0 as usize) == Some(&Type::Constructor(TypeConstructor::Int))
        })
}

/// Validate the physical view of the checked carrier after source ABI matching.
pub(crate) fn is_guest(guest: &crate::cc::GuestLayout) -> bool {
    use crate::cc::{GuestLayout, ValueShape};
    let (labels, fields) = match guest {
        GuestLayout::Product { labels, fields, .. }
        | GuestLayout::RecordStorage { labels, fields, .. } => (labels, fields),
        _ => return false,
    };
    labels.iter().map(String::as_str).eq(LABELS)
        && fields.len() == 2
        && fields.iter().all(|field| {
            field.stored == ValueShape::Integer && field.value.shape() == ValueShape::Integer
        })
}

/// The two little-endian memory words of a canonical i64. This view is only
/// used for memory copies; its eight-byte extent remains owned by the i64 ABI.
pub(crate) fn memory_view() -> crate::abi::canonical::CanonicalType {
    use crate::abi::canonical::{CanonicalField, CanonicalType};
    CanonicalType::Record(
        ["low", "high"]
            .map(|name| CanonicalField {
                name: name.into(),
                ty: CanonicalType::Int {
                    width: 32,
                    signed: false,
                },
            })
            .into(),
    )
}
