//! Identity-preserving CC operations, shared by semantic evidence consumers.
use super::{AssignmentKind, ValueConversion, ValueId};
use std::collections::HashSet;

pub(crate) fn reference_origin<'a>(
    mut value: ValueId,
    mut definition: impl FnMut(ValueId) -> Option<&'a AssignmentKind>,
) -> ValueId {
    let mut seen = HashSet::new();
    while seen.insert(value) {
        let source = match definition(value) {
            Some(AssignmentKind::RepresentationCast { value, .. }) => Some(*value),
            Some(AssignmentKind::AggregateConvert {
                value, conversion, ..
            }) if transparent(&conversion.plan) => Some(*value),
            _ => None,
        };
        match source {
            Some(source) => value = source,
            None => break,
        }
    }
    value
}

fn transparent(conversion: &ValueConversion) -> bool {
    match conversion {
        ValueConversion::Identity
        | ValueConversion::EraseReference
        | ValueConversion::RecoverReference { .. } => true,
        ValueConversion::Sequence(steps) => steps.iter().all(transparent),
        _ => false,
    }
}
