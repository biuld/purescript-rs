//! Source ABI shapes for WIT imports. CC sees [`ValueShape`], not WIT names.
//!
//! The declaration's resolved source type is interned in the Core type table
//! and carried as a `type_id`. CC derives the abstract signature directly from
//! that Core type, so a foreign signature shares the canonical layout of the
//! same type used elsewhere in the module.

use super::{RefShape, Reference, ReprId, Signature, ValueShape};
use psrs_core::{Module as CoreModule, Type as CoreType, TypeConstructor, TypeId};
use psrs_hir::{SymbolId, TypeId as HirTypeId};
use std::collections::HashMap;

pub(crate) fn abstract_signature(
    type_id: Option<TypeId>,
    module: &CoreModule,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    constructor_types: &HashMap<SymbolId, ReprId>,
) -> Option<Signature> {
    let type_reprs = type_representations(module, constructor_types);
    let (parameter_ids, result_id) = crate::abi::link::function_parts(module, type_id?)?;
    let mut parameters = Vec::with_capacity(parameter_ids.len());
    for parameter in parameter_ids {
        parameters.push(core_shape(
            module,
            parameter,
            record_types,
            array_types,
            &type_reprs,
        )?);
    }
    Some(Signature {
        parameters,
        result: core_shape(module, result_id, record_types, array_types, &type_reprs)?,
    })
}

/// Maps each payload-bearing data type declaration to the variant representation
/// the `TypeLayout` assigned to its constructors. Nullary enums, opaque handles,
/// and newtypes are absent.
fn type_representations(
    module: &CoreModule,
    constructor_types: &HashMap<SymbolId, ReprId>,
) -> HashMap<HirTypeId, ReprId> {
    module
        .constructors
        .iter()
        .filter_map(|constructor| {
            constructor_types
                .get(&constructor.symbol)
                .map(|repr| (constructor.type_id, *repr))
        })
        .collect()
}

fn core_shape(
    module: &CoreModule,
    id: TypeId,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    type_reprs: &HashMap<HirTypeId, ReprId>,
) -> Option<ValueShape> {
    Some(match module.types.get(id.0 as usize)? {
        CoreType::I32 | CoreType::Char | CoreType::Unit => ValueShape::Integer,
        // A nullary enum or an opaque handle is an `i32`; a payload-bearing data
        // type is a variant reference (DEC-13).
        CoreType::Constructor(TypeConstructor::User(type_id)) => match type_reprs.get(type_id) {
            Some(repr) => reference(*repr),
            None => ValueShape::Integer,
        },
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
        // A parameterized payload-bearing data type such as `Maybe T`.
        CoreType::Application(_, _) => match head_user_type(module, id) {
            Some(type_id) => reference(*type_reprs.get(&type_id)?),
            None => return None,
        },
        _ => return None,
    })
}

fn head_user_type(module: &CoreModule, mut id: TypeId) -> Option<HirTypeId> {
    loop {
        match module.types.get(id.0 as usize)? {
            CoreType::Constructor(TypeConstructor::User(type_id)) => return Some(*type_id),
            CoreType::Application(function, _) => id = *function,
            _ => return None,
        }
    }
}

fn reference(representation: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(representation),
    })
}
