//! Predeclared representations for the persistent control abstract machine.

mod contracts;

pub(in crate::cc::layout) use contracts::append as append_contracts;
pub(in crate::cc) use contracts::lookup;

use crate::cc::payload::erased_shape;
use crate::cc::{
    ReprId, Representation, RepresentationTable, Signature, SignatureId, ValueShape, VariantCase,
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
    pub run: SignatureId,
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
    let callback = crate::cc::layout::protocols::array(table, erased);
    let status = crate::cc::layout::protocols::array(table, ValueShape::Integer);
    let replay_result = table.reserve();
    let labels = vec!["_1".to_owned(), "_2".to_owned()];
    let pair = crate::cc::layout::protocols::record(table, &labels).unwrap_or_else(|| {
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
    let schema = crate::cc::ControlSchema {
        computation,
        sequence,
        context,
        extent: boundary,
        status,
        owner,
        callback,
        result_pair: pair,
        replay_result,
    };
    let shape = |kind| schema.shape(kind, continuation);
    use crate::control::schema::{
        ComputationCase, ContextCase, ControlHelperRole as R, ExtentCase, FieldKind, OwnerCase,
        SequenceCase,
    };
    for (repr, cases) in [
        (
            computation,
            ComputationCase::ALL
                .iter()
                .map(|case| (case.tag(), case.fields()))
                .collect::<Vec<_>>(),
        ),
        (
            sequence,
            SequenceCase::ALL
                .iter()
                .map(|case| (case.tag(), case.fields()))
                .collect(),
        ),
        (
            context,
            ContextCase::ALL
                .iter()
                .map(|case| (case.tag(), case.fields()))
                .collect(),
        ),
        (
            owner,
            OwnerCase::ALL
                .iter()
                .map(|case| (case.tag(), case.fields()))
                .collect(),
        ),
        (
            boundary,
            ExtentCase::ALL
                .iter()
                .map(|case| (case.tag(), case.fields()))
                .collect(),
        ),
    ] {
        table.set(
            repr,
            Representation::Variant {
                cases: cases
                    .into_iter()
                    .map(|(tag, fields)| VariantCase {
                        tag,
                        fields: fields.iter().map(|kind| shape(*kind)).collect(),
                    })
                    .collect(),
            },
        );
    }
    table.set(
        replay_result,
        Representation::Product {
            fields: vec![shape(FieldKind::Sequence), shape(FieldKind::Extent)],
        },
    );
    let mut signature = |role: R| {
        table.add_signature(Signature {
            parameters: role.parameters().iter().map(|kind| shape(*kind)).collect(),
            result: shape(role.result()),
        })
    };
    let executor = signature(R::Executor);
    let run = signature(R::Run);
    let clone_sequence = signature(R::CloneSequence);
    let rebuild_sequence = signature(R::RebuildSequence);
    let start = signature(R::Start);
    let boundary_check = signature(R::Check);
    let once = signature(R::Once);
    let cancel = signature(R::Cancel);
    ControlLayout {
        computation,
        sequence,
        pair,
        continuation,
        executor,
        run,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_cells_reuse_existing_canonical_array_layouts() {
        let mut table = RepresentationTable::default();
        let erased = erased_shape();
        let callback = crate::cc::layout::protocols::array(&mut table, erased);
        let status =
            crate::cc::layout::protocols::array(&mut table, crate::cc::ValueShape::Integer);
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
