//! Source ABI shapes for WIT imports. CC sees [`ValueShape`], not WIT names.
//!
//! The declaration's resolved source type is interned in the Core type table
//! and carried as a `type_id`. CC derives the abstract signature directly from
//! that Core type, so a foreign signature shares the canonical layout of the
//! same type used elsewhere in the module. The recursive guest layout is read
//! separately from the representation table through
//! [`super::representation::guest_layout`].

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
    let parameters = parameter_ids
        .into_iter()
        .map(|parameter| payload_shape(module, parameter, record_types, array_types, &type_reprs))
        .collect::<Option<Vec<_>>>()?;
    let result = payload_shape(module, result_id, record_types, array_types, &type_reprs)?;
    Some(Signature { parameters, result })
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

fn payload_shape(
    module: &CoreModule,
    id: TypeId,
    record_types: &HashMap<TypeId, ReprId>,
    array_types: &HashMap<TypeId, ReprId>,
    type_reprs: &HashMap<HirTypeId, ReprId>,
) -> Option<ValueShape> {
    if let Some(shape) = super::layout::primitive_shape_of(module, id) {
        return Some(shape);
    }
    Some(match module.types.get(id.0 as usize)? {
        CoreType::Application(_, _) if module.is_record_type(id) => {
            reference(*record_types.get(&id)?)
        }
        CoreType::Application(function, _)
            if matches!(
                module.types.get(function.0 as usize),
                Some(CoreType::Constructor(TypeConstructor::Array))
            ) =>
        {
            reference(*array_types.get(&id)?)
        }
        CoreType::Constructor(TypeConstructor::User(_)) | CoreType::Application(_, _) => {
            // A newtype is represented by its single field, so `Resource a`
            // erases to the `Int` handle index it wraps (DEC-14).
            match newtype_underlying(module, id) {
                Some(inner) => payload_shape(module, inner, record_types, array_types, type_reprs)?,
                None => user_payload_shape(module, id, type_reprs)?,
            }
        }
        _ => return None,
    })
}

/// The single field type of a newtype at its resolved application, or `None`
/// when the type is not a newtype.
fn newtype_underlying(module: &CoreModule, id: TypeId) -> Option<TypeId> {
    let head = head_user_type(module, id)?;
    super::layout::newtype_field_type(module, head)
}

/// The abstract shape of a source data type: an `i32` for a nullary enum or an
/// opaque handle, otherwise a reference to its variant representation.
fn user_payload_shape(
    module: &CoreModule,
    id: TypeId,
    type_reprs: &HashMap<HirTypeId, ReprId>,
) -> Option<ValueShape> {
    let head = head_user_type(module, id)?;
    let constructors = constructors_of(module, head);
    if constructors.is_empty() || constructors.iter().all(|case| case.field_count == 0) {
        return Some(ValueShape::Integer);
    }
    Some(reference(*type_reprs.get(&head)?))
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

fn constructors_of(module: &CoreModule, type_id: HirTypeId) -> Vec<&psrs_core::ConstructorInfo> {
    let mut constructors = module
        .constructors
        .iter()
        .filter(|constructor| constructor.type_id == type_id)
        .collect::<Vec<_>>();
    constructors.sort_by_key(|constructor| constructor.tag);
    constructors
}

fn reference(representation: ReprId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Repr(representation),
    })
}
