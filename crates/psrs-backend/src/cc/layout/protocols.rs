//! Aggregate owners' canonical protocols for bare polymorphic value slots.
use super::super::{RefShape, Reference, ReprId, Representation, RepresentationTable, ValueShape};

pub(super) fn append(table: &mut RepresentationTable, has_source_arrays: bool) {
    let erased = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    });
    // Internal record arrays have their own schema. Only source arrays require
    // the source Array and text-codec protocols.
    if has_source_arrays {
        array(table, erased);
        array(table, ValueShape::Integer);
    }
}

/// A complete typed product whose fields happen to be erased. This is not
/// the record owner's bare-slot protocol; the control result-pair ABI uses it.
pub(in crate::cc) fn erased_product(
    table: &RepresentationTable,
    labels: &[String],
) -> Option<ReprId> {
    let erased = crate::cc::payload::erased_shape();
    table.product_labels.iter().find_map(|(id, existing)| {
        (*existing == labels
            && matches!(table.representation(*id), Some(Representation::Product { fields })
                if fields.len() == labels.len() && fields.iter().all(|field| *field == erased)))
        .then_some(*id)
    })
}

/// Reuse the authoritative array representation for a physical element shape.
pub(in crate::cc) fn array(table: &mut RepresentationTable, element: ValueShape) -> ReprId {
    if let Some(id) = array_for(table, element) {
        return id;
    }
    let id = table.reserve();
    table.set(id, Representation::Array { element });
    id
}

pub(in crate::cc) fn array_for(table: &RepresentationTable, element: ValueShape) -> Option<ReprId> {
    table.representations.iter().position(|representation| {
        matches!(representation, Representation::Array { element: existing } if *existing == element)
    }).map(|index| ReprId(index as u32))
}
