//! Aggregate owners' canonical protocols for bare polymorphic value slots.
use super::super::{RefShape, Reference, ReprId, Representation, RepresentationTable, ValueShape};

pub(super) fn append(table: &mut RepresentationTable) {
    let erased = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    });
    // Register the canonical erased-element array only for a module that has an
    // array layout to normalize. A bare slot can hold an array only when a
    // concrete array type exists, so a program without arrays must not gain a
    // dead representation that would claim a newtype allocated storage.
    let has_array = table
        .representations
        .iter()
        .any(|representation| matches!(representation, Representation::Array { .. }));
    if has_array
        && !table.representations.iter().any(|representation|
        matches!(representation, Representation::Array { element } if *element == erased))
    {
        let id = table.reserve();
        table.set(id, Representation::Array { element: erased });
    }
    let labels = table
        .product_labels
        .values()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for labels in labels {
        if table.product_labels.iter().any(|(id, existing)| {
            *existing == labels
                && matches!(table.representation(*id), Some(Representation::Product { fields })
                if fields.len() == labels.len() && fields.iter().all(|field| *field == erased))
        }) {
            continue;
        }
        let id = table.reserve();
        table.set(
            id,
            Representation::Product {
                fields: vec![erased; labels.len()],
            },
        );
        table.set_product_labels(id, labels);
    }
}

pub(crate) fn record(table: &RepresentationTable, labels: &[String]) -> Option<ReprId> {
    let erased = ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    });
    table.product_labels.iter().find_map(|(id, existing)| {
        (*existing == labels
            && matches!(table.representation(*id), Some(Representation::Product { fields })
            if fields.len() == labels.len() && fields.iter().all(|field| *field == erased)))
        .then_some(*id)
    })
}
