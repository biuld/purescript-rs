#![allow(dead_code)]
//! Identity-preserving CC operations, shared by semantic evidence consumers.
use super::{AssignmentKind, ValueConversion, ValueId};
use std::collections::HashSet;

/// A join preserves identity only when every normal alternative has the same
/// origin. A trapping alternative contributes no returned value.
pub(crate) fn reference_origin<'a>(
    value: ValueId,
    mut definition: impl FnMut(ValueId) -> Option<&'a AssignmentKind>,
) -> ValueId {
    fn resolve<'a>(
        value: ValueId,
        definition: &mut impl FnMut(ValueId) -> Option<&'a AssignmentKind>,
        active: &mut HashSet<ValueId>,
    ) -> Option<ValueId> {
        if !active.insert(value) {
            return Some(value);
        }
        let result = match definition(value) {
            Some(AssignmentKind::Unreachable) => None,
            Some(
                AssignmentKind::RepresentationCast { value, .. }
                | AssignmentKind::ArraySet { value, .. },
            ) => resolve(*value, definition, active),
            Some(AssignmentKind::AggregateConvert {
                value, conversion, ..
            }) if transparent(&conversion.plan) => resolve(*value, definition, active),
            Some(AssignmentKind::If {
                then_value,
                else_value,
                ..
            }) => join(value, [*then_value, *else_value], definition, active),
            Some(AssignmentKind::TagSwitch {
                cases,
                default_value,
                ..
            }) => join(
                value,
                cases.iter().map(|case| case.value).chain([*default_value]),
                definition,
                active,
            ),
            _ => Some(value),
        };
        active.remove(&value);
        result
    }
    fn join<'a>(
        value: ValueId,
        inputs: impl IntoIterator<Item = ValueId>,
        definition: &mut impl FnMut(ValueId) -> Option<&'a AssignmentKind>,
        active: &mut HashSet<ValueId>,
    ) -> Option<ValueId> {
        let mut common = None;
        for input in inputs {
            if let Some(origin) = resolve(input, definition, active) {
                if common.is_some_and(|previous| previous != origin) {
                    return Some(value);
                }
                common = Some(origin);
            }
        }
        common
    }
    resolve(value, &mut definition, &mut HashSet::new()).unwrap_or(value)
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
            // An in-place store changes contents, not the carrier's definition.
            // Preserve the allocation/projection that established its identity.
            if !matches!(&assignment.kind, AssignmentKind::ArraySet { value, .. } if *value == assignment.destination)
            {
                definitions.insert(assignment.destination, &assignment.kind);
            }
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

pub(crate) fn path_origin(
    mut value: ValueId,
    definitions: &std::collections::HashMap<ValueId, &AssignmentKind>,
    aliases: &std::collections::HashMap<ValueId, ValueId>,
) -> ValueId {
    let mut seen = HashSet::new();
    while seen.insert(value) {
        let next =
            aliases
                .get(&value)
                .copied()
                .or_else(|| match definitions.get(&value).copied() {
                    Some(
                        AssignmentKind::RepresentationCast { value, .. }
                        | AssignmentKind::ArraySet { value, .. },
                    ) => Some(*value),
                    Some(AssignmentKind::AggregateConvert {
                        value, conversion, ..
                    }) if transparent(&conversion.plan) => Some(*value),
                    _ => None,
                });
        match next {
            Some(next) => value = next,
            None => break,
        }
    }
    value
}
