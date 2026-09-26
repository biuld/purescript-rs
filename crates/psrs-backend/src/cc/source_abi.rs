//! Source ABI shapes for WIT imports. CC sees [`ValueShape`], not WIT names.

use super::{
    RefShape, Reference, ReprId, Representation, RepresentationTable, Signature, ValueShape,
};
use crate::abi::{SourceSignature, SourceType};
use psrs_core::{Module as CoreModule, Type as CoreType, TypeConstructor, TypeId};
use psrs_hir::TypeId as HirTypeId;
use std::collections::HashMap;

pub(crate) fn abstract_signature(
    signature: &SourceSignature,
    module: &CoreModule,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    representations: &mut RepresentationTable,
) -> Option<Signature> {
    let mut parameters = Vec::with_capacity(signature.parameters.len());
    for parameter in &signature.parameters {
        parameters.push(scalar_source_type(
            parameter,
            module,
            record_types,
            array_types,
            representations,
        )?);
    }
    Some(Signature {
        parameters,
        result: scalar_source_type(
            &signature.result,
            module,
            record_types,
            array_types,
            representations,
        )?,
    })
}

fn scalar_source_type(
    ty: &SourceType,
    module: &CoreModule,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    representations: &mut RepresentationTable,
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
        SourceType::Record { .. } => {
            let type_id = module
                .types
                .iter()
                .enumerate()
                .find_map(|(index, core_type)| {
                    core_type_matches_source(module, core_type, ty).then_some(TypeId(index as u32))
                })?;
            let representation = record_types.get(&type_id).copied()?;
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(representation),
            })
        }
        SourceType::Array { element } => {
            let representation =
                array_representation(module, element, array_types, representations)?;
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(representation),
            })
        }
    })
}

/// Uses the module's canonical array when the element type is already interned.
/// An unused import has no core type yet, so it reserves one array of the same
/// element shape; a later call site still shares the interned handle.
fn array_representation(
    module: &CoreModule,
    element: &SourceType,
    array_types: &HashMap<TypeId, ReprId>,
    representations: &mut RepresentationTable,
) -> Option<ReprId> {
    let shape = SourceType::Array {
        element: Box::new(element.clone()),
    };
    if let Some(type_id) = module.types.iter().enumerate().find_map(|(index, core)| {
        core_type_matches_source(module, core, &shape).then_some(TypeId(index as u32))
    }) {
        return array_types.get(&type_id).copied();
    }
    let element = array_element_shape(element)?;
    let id = representations.reserve();
    representations.set(id, Representation::Array { element });
    Some(id)
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

fn core_type_matches_source(module: &CoreModule, core: &CoreType, source: &SourceType) -> bool {
    match (core, source) {
        (CoreType::I32, SourceType::Int)
        | (CoreType::Boolean, SourceType::Boolean)
        | (CoreType::F64, SourceType::Number)
        | (CoreType::Char, SourceType::Char)
        | (CoreType::String, SourceType::String)
        | (CoreType::Unit, SourceType::Unit) => true,
        (CoreType::Constructor(TypeConstructor::User(type_id)), SourceType::Enum { cases }) => {
            source_enum_cases(module, *type_id).as_ref() == Some(cases)
        }
        (CoreType::Application(function, _), SourceType::Enum { cases }) => {
            core_type_user_id(module, *function)
                .and_then(|type_id| source_enum_cases(module, type_id))
                .as_ref()
                == Some(cases)
        }
        (CoreType::Application(function, argument), SourceType::Array { element }) => {
            matches!(
                module.types.get(function.0 as usize),
                Some(CoreType::Constructor(TypeConstructor::Array))
            ) && module
                .types
                .get(argument.0 as usize)
                .is_some_and(|core| core_type_matches_source(module, core, element))
        }
        (CoreType::Record(core_fields), SourceType::Record { fields }) => {
            core_fields.len() == fields.len()
                && core_fields.iter().zip(fields).all(
                    |((core_label, core_type), (source_label, source_type))| {
                        core_label == source_label
                            && module.types.get(core_type.0 as usize).is_some_and(|core| {
                                core_type_matches_source(module, core, source_type)
                            })
                    },
                )
        }
        _ => false,
    }
}

fn core_type_user_id(module: &CoreModule, id: TypeId) -> Option<HirTypeId> {
    match module.types.get(id.0 as usize)? {
        CoreType::Constructor(TypeConstructor::User(type_id)) => Some(*type_id),
        CoreType::Application(function, _) => core_type_user_id(module, *function),
        _ => None,
    }
}

fn source_enum_cases(module: &CoreModule, type_id: HirTypeId) -> Option<Vec<String>> {
    let mut constructors = module
        .constructors
        .iter()
        .filter(|constructor| constructor.type_id == type_id)
        .collect::<Vec<_>>();
    constructors.sort_by_key(|constructor| constructor.tag);
    if constructors.is_empty()
        || constructors.iter().enumerate().any(|(index, constructor)| {
            constructor.tag != index as u32 || constructor.field_count != 0
        })
    {
        return None;
    }
    Some(
        constructors
            .into_iter()
            .map(|constructor| constructor.name.clone())
            .collect(),
    )
}
