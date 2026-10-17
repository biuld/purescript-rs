//! Bridge the private machine pair and the public erased record protocol.
use crate::cc::payload::{PayloadPlanner, StoragePayloadPlanner, erased_shape};
use crate::cc::{AggregateConvert, ReprId, RepresentationTable};

pub(super) fn conversion(
    table: &RepresentationTable,
    pair: ReprId,
    entering: bool,
) -> AggregateConvert {
    let shape = super::builder::repr(pair);
    let mut planner = StoragePayloadPlanner(table);
    let span = psrs_span::TextRange::default();
    let (source, destination, plan) = if entering {
        (shape, erased_shape(), planner.erase_payload(shape, span))
    } else {
        (erased_shape(), shape, planner.recover_payload(shape, span))
    };
    AggregateConvert {
        source,
        destination,
        plan: plan.expect("machine result pair must have a record storage protocol"),
    }
}
