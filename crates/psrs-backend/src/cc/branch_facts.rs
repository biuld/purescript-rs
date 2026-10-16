#![allow(dead_code)]
//! Immutable variant tag facts on structured CC paths.
use super::{AssignmentKind as A, ReprId, ValueId};
use std::collections::HashMap;

pub(crate) type Facts = HashMap<(ReprId, ValueId), i32>;
pub(crate) type Definitions<'a> = HashMap<ValueId, &'a A>;

pub(crate) fn tag_source(
    value: ValueId,
    definitions: &Definitions<'_>,
) -> Option<(ReprId, ValueId)> {
    let origin = |value| {
        super::value_identity::reference_origin(value, |value| definitions.get(&value).copied())
    };
    match definitions.get(&origin(value)).copied()? {
        A::VariantTag {
            representation,
            value,
            ..
        } => Some((*representation, origin(*value))),
        _ => None,
    }
}

pub(crate) fn selected(
    representation: ReprId,
    value: ValueId,
    facts: &Facts,
    definitions: &Definitions<'_>,
) -> Option<i32> {
    let value =
        super::value_identity::reference_origin(value, |value| definitions.get(&value).copied());
    match definitions.get(&value).copied() {
        Some(A::VariantNew {
            representation: actual,
            case,
            ..
        }) if *actual == representation => i32::try_from(*case).ok(),
        _ => facts.get(&(representation, value)).copied(),
    }
}

/// Equality established by a selected Boolean branch, including swapped
/// operands. Unknown inequalities do not become fabricated scalar facts.
pub(crate) fn equality_selection(
    condition: ValueId,
    truth: bool,
    definitions: &Definitions<'_>,
    aliases: &HashMap<ValueId, ValueId>,
) -> Option<(ValueId, i32)> {
    let origin = |value| super::value_identity::path_origin(value, definitions, aliases);
    let A::Primitive { op, left, right } = definitions.get(&origin(condition)).copied()? else {
        return None;
    };
    if !matches!(
        (op, truth),
        (super::BinaryOp::IntEq, true) | (super::BinaryOp::IntNe, false)
    ) {
        return None;
    }
    for (value, constant) in [(*left, *right), (*right, *left)] {
        if let Some(A::Constant(constant)) = definitions.get(&origin(constant)).copied() {
            return Some((origin(value), *constant));
        }
    }
    None
}
