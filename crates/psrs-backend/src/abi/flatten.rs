//! Flat canonical slots for a WIT parameter list.
//!
//! `option`, `result`, tuples, and payload-bearing variants are one WIT
//! parameter and several core values. A primitive foreign import names those
//! values (`Int` for a discriminant or handle, `String` for `(pointer, length)`)
//! instead of a compiler aggregate. `Char` stays distinct from `Int` and from
//! a handle even though all three are `i32` on the wire.

#[cfg(test)]
use super::SourceType;
use crate::types::ValueType;
use wit_parser::{Int, Resolve, Type as WitType, TypeDefKind};

/// One core slot in a WIT parameter's canonical flattening, excluding a return
/// pointer. The variant records the source primitive that may fill the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlatSlot {
    Int32,
    Int64 {
        signed: bool,
    },
    Boolean,
    Char,
    Float32,
    Float64,
    Handle,
    Pointer,
    Length,
    /// Joined variant payloads that are not one source primitive.
    Ambiguous,
}

#[cfg(test)]
pub(crate) fn is_primitive(ty: &SourceType) -> bool {
    matches!(
        ty,
        SourceType::Int
            | SourceType::Boolean
            | SourceType::Number
            | SourceType::Char
            | SourceType::String
            | SourceType::Unit
    )
}

#[cfg(test)]
pub(crate) fn is_primitive_signature(parameters: &[SourceType], result: &SourceType) -> bool {
    is_primitive(result) && parameters.iter().all(is_primitive)
}

/// Whether an otherwise unsupported WIT parameter is only scalars, handles,
/// byte lists, and `option` / `result` / tuple / variant structure around them.
/// Maps, futures, streams, and non-byte lists are not.
pub(crate) fn primitive_aggregate_allowed(resolve: &Resolve, ty: &WitType) -> bool {
    match ty {
        WitType::Bool
        | WitType::S8
        | WitType::U8
        | WitType::S16
        | WitType::U16
        | WitType::S32
        | WitType::U32
        | WitType::S64
        | WitType::U64
        | WitType::F32
        | WitType::F64
        | WitType::Char
        | WitType::String => true,
        WitType::ErrorContext => false,
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::Type(inner) => primitive_aggregate_allowed(resolve, inner),
            TypeDefKind::Handle(_) | TypeDefKind::Enum(_) | TypeDefKind::Flags(_) => true,
            TypeDefKind::List(inner) | TypeDefKind::FixedLengthList(inner, _) => {
                list_element_is_u8(resolve, inner)
            }
            TypeDefKind::Record(record) => record
                .fields
                .iter()
                .all(|field| primitive_aggregate_allowed(resolve, &field.ty)),
            TypeDefKind::Tuple(tuple) => tuple
                .types
                .iter()
                .all(|ty| primitive_aggregate_allowed(resolve, ty)),
            TypeDefKind::Option(inner) => primitive_aggregate_allowed(resolve, inner),
            TypeDefKind::Result(result) => {
                result
                    .ok
                    .as_ref()
                    .is_none_or(|ty| primitive_aggregate_allowed(resolve, ty))
                    && result
                        .err
                        .as_ref()
                        .is_none_or(|ty| primitive_aggregate_allowed(resolve, ty))
            }
            TypeDefKind::Variant(variant) => variant.cases.iter().all(|case| {
                case.ty
                    .as_ref()
                    .is_none_or(|ty| primitive_aggregate_allowed(resolve, ty))
            }),
            TypeDefKind::Resource
            | TypeDefKind::Map(_, _)
            | TypeDefKind::Future(_)
            | TypeDefKind::Stream(_)
            | TypeDefKind::Unknown => false,
        },
    }
}

fn list_element_is_u8(resolve: &Resolve, ty: &WitType) -> bool {
    match ty {
        WitType::U8 => true,
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::Type(inner) => list_element_is_u8(resolve, inner),
            _ => false,
        },
        _ => false,
    }
}

/// Flattens every WIT parameter, in order, to canonical slots. This is not the
/// indirect-parameter collapse: a signature that passes a pointer to a
/// parameter record still yields the direct slots here, and validation rejects
/// that shape for a primitive import.
pub(crate) fn flatten_parameters(
    resolve: &Resolve,
    function: &wit_parser::Function,
) -> Vec<FlatSlot> {
    let mut slots = Vec::new();
    for parameter in &function.params {
        push_type(resolve, &parameter.ty, &mut slots);
    }
    slots
}

pub(crate) fn slot_value_type(slot: &FlatSlot) -> Option<ValueType> {
    Some(match slot {
        FlatSlot::Int32
        | FlatSlot::Boolean
        | FlatSlot::Char
        | FlatSlot::Handle
        | FlatSlot::Pointer
        | FlatSlot::Length => ValueType::I32,
        FlatSlot::Int64 { .. } => ValueType::I64,
        FlatSlot::Float32 => ValueType::F32,
        FlatSlot::Float64 => ValueType::F64,
        FlatSlot::Ambiguous => return None,
    })
}

/// Walks `parameters` against `slots`. `String` consumes `(pointer, length)`.
/// `Int` matches an integer slot or a handle, not a `Char` or a `Boolean`.
#[cfg(test)]
pub(crate) fn parameters_match(parameters: &[SourceType], slots: &[FlatSlot]) -> bool {
    let mut index = 0;
    for parameter in parameters {
        if !consume(parameter, slots, &mut index) {
            return false;
        }
    }
    index == slots.len()
}

#[cfg(test)]
fn consume(parameter: &SourceType, slots: &[FlatSlot], index: &mut usize) -> bool {
    let Some(slot) = slots.get(*index) else {
        return false;
    };
    match parameter {
        SourceType::Int => matches!(
            slot,
            FlatSlot::Int32 | FlatSlot::Int64 { .. } | FlatSlot::Handle
        ),
        SourceType::Boolean => matches!(slot, FlatSlot::Boolean),
        SourceType::Char => matches!(slot, FlatSlot::Char),
        SourceType::Number => matches!(slot, FlatSlot::Float32 | FlatSlot::Float64),
        SourceType::String => {
            let length = slots.get(*index + 1);
            if matches!((slot, length), (FlatSlot::Pointer, Some(FlatSlot::Length))) {
                *index += 2;
                return true;
            }
            return false;
        }
        SourceType::Unit
        | SourceType::Enum { .. }
        | SourceType::Record { .. }
        | SourceType::Resource { .. }
        | SourceType::Array { .. } => {
            return false;
        }
    }
    .then(|| {
        *index += 1;
    })
    .is_some()
}

fn push_type(resolve: &Resolve, ty: &WitType, slots: &mut Vec<FlatSlot>) {
    match ty {
        WitType::Bool => slots.push(FlatSlot::Boolean),
        WitType::S8 | WitType::U8 | WitType::S16 | WitType::U16 | WitType::S32 | WitType::U32 => {
            slots.push(FlatSlot::Int32);
        }
        WitType::Char => slots.push(FlatSlot::Char),
        WitType::U64 => slots.push(FlatSlot::Int64 { signed: false }),
        WitType::S64 => slots.push(FlatSlot::Int64 { signed: true }),
        WitType::F32 => slots.push(FlatSlot::Float32),
        WitType::F64 => slots.push(FlatSlot::Float64),
        WitType::String => {
            slots.push(FlatSlot::Pointer);
            slots.push(FlatSlot::Length);
        }
        WitType::ErrorContext => slots.push(FlatSlot::Ambiguous),
        WitType::Id(id) => match &resolve.types[*id].kind {
            TypeDefKind::Type(inner) => push_type(resolve, inner, slots),
            TypeDefKind::Handle(_) => slots.push(FlatSlot::Handle),
            TypeDefKind::Resource => slots.push(FlatSlot::Ambiguous),
            TypeDefKind::Record(record) => {
                for field in &record.fields {
                    push_type(resolve, &field.ty, slots);
                }
            }
            TypeDefKind::Tuple(tuple) => {
                for ty in &tuple.types {
                    push_type(resolve, ty, slots);
                }
            }
            TypeDefKind::Flags(flags) => {
                for _ in 0..flags.repr().count() {
                    slots.push(FlatSlot::Int32);
                }
            }
            TypeDefKind::List(_) => {
                slots.push(FlatSlot::Pointer);
                slots.push(FlatSlot::Length);
            }
            TypeDefKind::FixedLengthList(element, size) => {
                for _ in 0..*size {
                    push_type(resolve, element, slots);
                }
            }
            TypeDefKind::Enum(enum_) => push_tag(enum_.tag(), slots),
            TypeDefKind::Option(payload) => {
                slots.push(FlatSlot::Int32);
                push_cases(resolve, &[None, Some(payload)], slots);
            }
            TypeDefKind::Result(result) => {
                slots.push(FlatSlot::Int32);
                push_cases(resolve, &[result.ok.as_ref(), result.err.as_ref()], slots);
            }
            TypeDefKind::Variant(variant) => {
                push_tag(variant.tag(), slots);
                let cases = variant
                    .cases
                    .iter()
                    .map(|case| case.ty.as_ref())
                    .collect::<Vec<_>>();
                push_cases(resolve, &cases, slots);
            }
            TypeDefKind::Map(_, _) | TypeDefKind::Future(_) | TypeDefKind::Stream(_) => {
                slots.push(FlatSlot::Ambiguous);
            }
            TypeDefKind::Unknown => slots.push(FlatSlot::Ambiguous),
        },
    }
}

fn push_tag(tag: Int, slots: &mut Vec<FlatSlot>) {
    match tag {
        Int::U64 => slots.push(FlatSlot::Int64 { signed: false }),
        Int::U8 | Int::U16 | Int::U32 => slots.push(FlatSlot::Int32),
    }
}

/// Merges each case's payload the way the canonical ABI joins variant
/// payloads. An empty case contributes no slots. Distinct source kinds in the
/// same slot become [`FlatSlot::Ambiguous`] so a primitive import is rejected.
fn push_cases(resolve: &Resolve, cases: &[Option<&WitType>], slots: &mut Vec<FlatSlot>) {
    let mut merged = Vec::new();
    for case in cases {
        let Some(ty) = case else {
            continue;
        };
        let mut payload = Vec::new();
        push_type(resolve, ty, &mut payload);
        for (index, slot) in payload.into_iter().enumerate() {
            if let Some(existing) = merged.get_mut(index) {
                if *existing != slot {
                    *existing = FlatSlot::Ambiguous;
                }
            } else {
                merged.push(slot);
            }
        }
    }
    slots.extend(merged);
}
