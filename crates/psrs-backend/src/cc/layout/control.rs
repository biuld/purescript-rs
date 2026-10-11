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
    pub owner: ReprId,
    pub callback: ReprId,
    pub boundary: ReprId,
    pub status: ReprId,
    pub start: SignatureId,
    pub boundary_check: SignatureId,
}

pub(super) fn append(table: &mut RepresentationTable, continuation: SignatureId) -> ControlLayout {
    let erased = erased_shape();
    let computation = table.reserve();
    table.set(
        computation,
        Representation::Variant {
            cases: [1, 1, 2, 3, 5, 6, 2]
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
        parameters: vec![erased; 5],
        result: erased,
    });
    let open_executor = table.add_signature(Signature {
        parameters: vec![erased; 7],
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
    let callback = super::protocols::array(table, erased);
    let owner = table.reserve();
    table.set(
        owner,
        Representation::Variant {
            cases: vec![VariantCase {
                tag: 0,
                fields: vec![erased; 2],
            }],
        },
    );
    let status = super::protocols::array(table, crate::cc::ValueShape::Integer);
    let boundary = table.reserve();
    table.set(
        boundary,
        Representation::Variant {
            cases: (0..2)
                .map(|tag| VariantCase {
                    tag,
                    fields: vec![erased; tag as usize + 1],
                })
                .collect(),
        },
    );
    let start = table.add_signature(Signature {
        parameters: vec![],
        result: erased,
    });
    let boundary_check = table.add_signature(Signature {
        parameters: vec![erased],
        result: erased,
    });
    ControlLayout {
        computation,
        sequence,
        pair,
        continuation,
        executor,
        open_executor,
        projected,
        owner,
        callback,
        boundary,
        status,
        start,
        boundary_check,
    }
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
        let layout = append(&mut table, continuation);
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
