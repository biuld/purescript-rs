//! Source ABI shapes for WIT imports. CC sees [`ValueShape`], not WIT names.
//!
//! The declaration's resolved source type is interned in the Core type table
//! and carried as a `type_id`. CC derives the abstract signature directly from
//! that Core type, so a foreign signature shares the canonical layout of the
//! same type used elsewhere in the module.

use super::{RefShape, Reference, ReprId, Signature, ValueShape};
use psrs_core::{Module as CoreModule, Type as CoreType, TypeConstructor, TypeId};
use std::collections::HashMap;

pub(crate) fn abstract_signature(
    type_id: Option<TypeId>,
    module: &CoreModule,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
) -> Option<Signature> {
    let (parameter_ids, result_id) = crate::abi::link::function_parts(module, type_id?)?;
    let mut parameters = Vec::with_capacity(parameter_ids.len());
    for parameter in parameter_ids {
        parameters.push(core_shape(module, parameter, record_types, array_types)?);
    }
    Some(Signature {
        parameters,
        result: core_shape(module, result_id, record_types, array_types)?,
    })
}

fn core_shape(
    module: &CoreModule,
    id: TypeId,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
) -> Option<ValueShape> {
    Some(match module.types.get(id.0 as usize)? {
        CoreType::I32
        | CoreType::Char
        | CoreType::Unit
        | CoreType::Constructor(TypeConstructor::User(_)) => ValueShape::Integer,
        CoreType::Boolean => ValueShape::Boolean,
        CoreType::F64 => ValueShape::Number,
        CoreType::String => ValueShape::String,
        CoreType::Record(_) => reference(record_types.get(&id).copied()?),
        CoreType::Application(function, _)
            if matches!(
                module.types.get(function.0 as usize),
                Some(CoreType::Constructor(TypeConstructor::Array))
            ) =>
        {
            reference(array_types.get(&id).copied()?)
        }
        _ => return None,
    })
}

fn reference(representation: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(representation),
    })
}
