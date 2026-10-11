//! Predeclared representations for the persistent control abstract machine.

use crate::cc::payload::erased_shape;
use crate::cc::{ReprId, Representation, RepresentationTable, Signature, SignatureId, VariantCase};

#[derive(Clone, Copy)]
pub(in crate::cc) struct ControlLayout {
    pub computation: ReprId,
    pub sequence: ReprId,
    pub pair: ReprId,
    pub continuation: SignatureId,
    pub executor: SignatureId,
    pub open_executor: SignatureId,
    pub projected: ReprId,
}

pub(super) fn append(table: &mut RepresentationTable, continuation: SignatureId) -> ControlLayout {
    let erased = erased_shape();
    let computation = table.reserve();
    table.set(
        computation,
        Representation::Variant {
            cases: [1, 1, 2, 2, 5]
                .into_iter()
                .enumerate()
                .map(|(tag, count)| VariantCase {
                    tag: tag as u32,
                    fields: vec![erased; count],
                })
                .collect(),
        },
    );
    let sequence = table.reserve();
    table.set(
        sequence,
        Representation::Variant {
            cases: [0, 2, 2]
                .into_iter()
                .enumerate()
                .map(|(tag, count)| VariantCase {
                    tag: tag as u32,
                    fields: vec![erased; count],
                })
                .collect(),
        },
    );
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
        parameters: vec![erased; 4],
        result: erased,
    });
    let open_executor = table.add_signature(Signature {
        parameters: vec![erased; 6],
        result: erased,
    });
    let projected = table.reserve();
    table.set(
        projected,
        Representation::Variant {
            cases: (0..2)
                .map(|tag| VariantCase {
                    tag,
                    fields: vec![erased],
                })
                .collect(),
        },
    );
    ControlLayout {
        computation,
        sequence,
        pair,
        continuation,
        executor,
        open_executor,
        projected,
    }
}
