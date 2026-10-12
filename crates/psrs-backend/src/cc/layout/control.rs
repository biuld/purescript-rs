//! Predeclared representations for the persistent control abstract machine.

use crate::cc::payload::erased_shape;
use crate::cc::{
    RefShape, Reference, ReprId, Representation, RepresentationTable, Signature, SignatureId,
    ValueShape, VariantCase,
};

use psrs_hir::TypeId as HirTypeId;
use std::collections::HashMap;

/// Reserve builtin carriers before any source signature or aggregate layout.
pub(in crate::cc) fn reserve(table: &mut RepresentationTable) -> HashMap<HirTypeId, ReprId> {
    HashMap::from([
        (HirTypeId::CONTROL, table.reserve()),
        (HirTypeId::RESUMPTION, table.reserve()),
    ])
}

#[derive(Clone, Copy)]
pub(in crate::cc) struct ControlLayout {
    pub computation: ReprId,
    pub sequence: ReprId,
    pub pair: ReprId,
    pub continuation: SignatureId,
    pub executor: SignatureId,
    pub context: ReprId,
    pub clone_sequence: SignatureId,
    pub rebuild_sequence: SignatureId,
    pub owner: ReprId,
    pub callback: ReprId,
    pub boundary: ReprId,
    pub status: ReprId,
    pub start: SignatureId,
    pub boundary_check: SignatureId,
    pub once: SignatureId,
    pub cancel: SignatureId,
    pub replay_result: ReprId,
}

pub(in crate::cc) fn append(
    table: &mut RepresentationTable,
    continuation: SignatureId,
    nominal_types: &HashMap<HirTypeId, ReprId>,
) -> ControlLayout {
    let erased = erased_shape();
    let computation = nominal_types[&HirTypeId::CONTROL];
    let sequence = table.reserve();
    let context = table.reserve();
    let owner = nominal_types[&HirTypeId::RESUMPTION];
    let boundary = table.reserve();
    let callback = super::protocols::array(table, erased);
    let status = super::protocols::array(table, ValueShape::Integer);
    let callable = reference(RefShape::Closure(continuation));
    let queue = reference(RefShape::Repr(sequence));
    let extent = reference(RefShape::Repr(boundary));
    let frame = reference(RefShape::Repr(computation));
    set_cases(
        table,
        computation,
        vec![
            vec![erased],
            vec![erased],
            vec![frame, callable],
            vec![erased, queue, extent],
            vec![callable, callable, frame],
            vec![], // Retired v1 Reenter tag; the executor rejects it.
            vec![reference(RefShape::Repr(owner)), erased],
        ],
    );
    set_cases(
        table,
        sequence,
        vec![
            vec![],
            vec![callable, queue],
            vec![queue, queue],
            vec![extent, queue, extent],
        ],
    );
    set_cases(
        table,
        context,
        vec![
            vec![extent],
            vec![
                callable,
                callable,
                queue,
                extent,
                reference(RefShape::Repr(context)),
                extent,
            ],
        ],
    );
    set_cases(
        table,
        owner,
        vec![vec![
            reference(RefShape::Repr(callback)),
            reference(RefShape::Repr(status)),
        ]],
    );
    set_cases(
        table,
        boundary,
        vec![
            vec![reference(RefShape::Repr(status))],
            vec![reference(RefShape::Repr(status)), extent],
        ],
    );
    let replay_result = table.reserve();
    table.set(
        replay_result,
        Representation::Product {
            fields: vec![queue, extent],
        },
    );
    let replay = reference(RefShape::Repr(replay_result));
    let labels = vec!["_1".to_owned(), "_2".to_owned()];
    let pair = super::protocols::record(table, &labels).unwrap_or_else(|| {
        let pair = table.reserve();
        table.set(
            pair,
            Representation::Product {
                fields: vec![erased; 2],
            },
        );
        table.set_product_labels(pair, labels);
        pair
    });
    let executor = table.add_signature(Signature {
        parameters: vec![
            erased,
            callable,
            frame,
            queue,
            extent,
            reference(RefShape::Repr(context)),
        ],
        result: erased,
    });
    let clone_sequence = table.add_signature(Signature {
        parameters: vec![queue, queue, queue, extent, extent],
        result: replay,
    });
    let rebuild_sequence = table.add_signature(Signature {
        parameters: vec![queue, queue, extent, extent],
        result: replay,
    });
    let start = table.add_signature(Signature {
        parameters: vec![],
        result: extent,
    });
    let boundary_check = table.add_signature(Signature {
        parameters: vec![extent],
        result: extent,
    });
    let owner_shape = reference(RefShape::Repr(owner));
    let once = table.add_signature(Signature {
        parameters: vec![callable],
        result: owner_shape,
    });
    let cancel = table.add_signature(Signature {
        parameters: vec![owner_shape],
        result: owner_shape,
    });
    ControlLayout {
        computation,
        sequence,
        pair,
        continuation,
        executor,
        context,
        clone_sequence,
        rebuild_sequence,
        owner,
        callback,
        boundary,
        status,
        start,
        boundary_check,
        once,
        cancel,
        replay_result,
    }
}

fn reference(heap: RefShape) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap,
    })
}

fn set_cases(table: &mut RepresentationTable, id: ReprId, fields: Vec<Vec<ValueShape>>) {
    table.set(
        id,
        Representation::Variant {
            cases: fields
                .into_iter()
                .enumerate()
                .map(|(tag, fields)| VariantCase {
                    tag: tag as u32,
                    fields,
                })
                .collect(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_cells_reuse_existing_canonical_array_layouts() {
        let mut table = RepresentationTable::default();
        let erased = erased_shape();
        let callback = super::super::protocols::array(&mut table, erased);
        let status = super::super::protocols::array(&mut table, crate::cc::ValueShape::Integer);
        let continuation = table.add_signature(Signature {
            parameters: vec![erased],
            result: erased,
        });
        let nominal_types = reserve(&mut table);
        let layout = append(&mut table, continuation, &nominal_types);
        assert_eq!(layout.callback, callback);
        assert_eq!(layout.status, status);
        assert_eq!(
            table
                .representations
                .iter()
                .filter(|repr| matches!(repr, Representation::Array { .. }))
                .count(),
            2
        );
    }
}
