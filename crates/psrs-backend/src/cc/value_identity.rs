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

pub(crate) fn transparent(conversion: &ValueConversion) -> bool {
    match conversion {
        ValueConversion::Identity
        | ValueConversion::EraseReference
        | ValueConversion::RecoverReference { .. } => true,
        ValueConversion::Sequence(steps) => steps.iter().all(transparent),
        _ => false,
    }
}

/// Index definitions across every nested assignment sequence, without guessing
/// identities from spans or names.
pub(crate) fn definitions(
    body: &[super::Assignment],
) -> std::collections::HashMap<ValueId, &AssignmentKind> {
    let mut definitions = std::collections::HashMap::new();
    let mut pending = vec![body];
    while let Some(body) = pending.pop() {
        for assignment in body {
            definitions.insert(assignment.destination, &assignment.kind);
            match &assignment.kind {
                AssignmentKind::If {
                    then_assignments,
                    else_assignments,
                    ..
                } => {
                    pending.extend([then_assignments.as_slice(), else_assignments.as_slice()]);
                }
                AssignmentKind::TagSwitch {
                    cases,
                    default_assignments,
                    ..
                } => {
                    pending.extend(cases.iter().map(|case| case.assignments.as_slice()));
                    pending.push(default_assignments);
                }
                _ => {}
            }
        }
    }
    definitions
}
