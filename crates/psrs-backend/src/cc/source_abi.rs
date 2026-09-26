//! Source ABI shapes for WIT imports. CC sees [`ValueShape`], not WIT names.
//!
//! The declaration's resolved source type is interned in the Core type table
//! and carried as a `type_id`. CC derives the record and array representations
//! from that identity, so a foreign signature shares the canonical layout of
//! the same type used elsewhere in the module.

use super::{
    RefShape, Reference, ReprId, Representation, RepresentationTable, Signature, ValueShape,
};
use crate::abi::{SourceSignature, SourceType};
use psrs_core::{Module as CoreModule, Type as CoreType, TypeId};

pub(crate) fn abstract_signature(
    signature: &SourceSignature,
    type_id: Option<TypeId>,
    module: &CoreModule,
    record_types: &std::collections::HashMap<TypeId, ReprId>,
    array_types: &std::collections::HashMap<TypeId, ReprId>,
    _representations: &mut RepresentationTable,
) -> Option<Signature> {
    let (parameter_ids, result_id) = split_function(module, type_id?)?;
    if parameter_ids.len() != signature.parameters.len() {
        return None;
    }
    let mut parameters = Vec::with_capacity(signature.parameters.len());
    for (parameter, core_id) in signature.parameters.iter().zip(&parameter_ids) {
        parameters.push(scalar_source_type(
            parameter,
            *core_id,
            record_types,
            array_types,
        )?);
    }
    Some(Signature {
        parameters,
        result: scalar_source_type(&signature.result, result_id, record_types, array_types)?,
    })
}

/// Walks a Core function type into its parameter types and final result type.
fn split_function(module: &CoreModule, type_id: TypeId) -> Option<(Vec<TypeId>, TypeId)> {
    let mut parameters = Vec::new();
    let mut current = type_id;
    loop {
        match module.types.get(current.0 as usize)? {
            CoreType::Function { parameter, result } => {
                parameters.push(*parameter);
                current = *result;
            }
            _ => return Some((parameters, current)),
        }
    }
}

fn scalar_source_type(
    ty: &SourceType,
    core_id: TypeId,
    record_types: &std::collections::HashMap<TypeId, ReprId>,
    array_types: &std::collections::HashMap<TypeId, ReprId>,
) -> Option<ValueShape> {
    Some(match ty {
        SourceType::Int
        | SourceType::Char
        | SourceType::Enum { .. }
        | SourceType::Unit
        | SourceType::Resource { .. } => ValueShape::Integer,
        SourceType::String => ValueShape::String,
        SourceType::Boolean => ValueShape::Boolean,
        SourceType::Number => ValueShape::Number,
        SourceType::Record { .. } => ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(record_types.get(&core_id).copied()?),
        }),
        SourceType::Array { element } => {
            array_element_shape(element)?;
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(array_types.get(&core_id).copied()?),
            })
        }
    })
}

fn array_element_shape(element: &SourceType) -> Option<ValueShape> {
    Some(match element {
        SourceType::Int | SourceType::Char => ValueShape::Integer,
        SourceType::Boolean => ValueShape::Boolean,
        SourceType::Number => ValueShape::Number,
        SourceType::String => ValueShape::String,
        _ => return None,
    })
}

pub(crate) fn signature_matches_source(
    source: &SourceSignature,
    actual: &Signature,
    representations: &RepresentationTable,
) -> bool {
    source.parameters.len() == actual.parameters.len()
        && source
            .parameters
            .iter()
            .zip(&actual.parameters)
            .all(|(source, actual)| source_shape_matches(source, actual, representations))
        && source_shape_matches(&source.result, &actual.result, representations)
}

fn source_shape_matches(
    source: &SourceType,
    actual: &ValueShape,
    representations: &RepresentationTable,
) -> bool {
    match source {
        SourceType::Int
        | SourceType::Char
        | SourceType::Enum { .. }
        | SourceType::Unit
        | SourceType::Resource { .. } => *actual == ValueShape::Integer,
        SourceType::String => *actual == ValueShape::String,
        SourceType::Boolean => *actual == ValueShape::Boolean,
        SourceType::Number => *actual == ValueShape::Number,
        SourceType::Record { fields } => {
            let ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(representation),
            }) = actual
            else {
                return false;
            };
            let Some(Representation::Product {
                fields: actual_fields,
            }) = representations.representation(*representation)
            else {
                return false;
            };
            fields.len() == actual_fields.len()
                && fields
                    .iter()
                    .zip(actual_fields)
                    .all(|((_, source), actual)| {
                        source_shape_matches(source, actual, representations)
                    })
        }
        SourceType::Array { element } => {
            let ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(representation),
            }) = actual
            else {
                return false;
            };
            let Some(Representation::Array {
                element: actual_element,
            }) = representations.representation(*representation)
            else {
                return false;
            };
            array_element_shape(element).is_some_and(|shape| shape == *actual_element)
        }
    }
}
