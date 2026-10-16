//! Complete callable storage templates of checked transparent newtypes.
//!
//! A newtype's canonical field keeps its declared erased representation.
//! Constructor arguments do not specialize that physical storage convention.
use psrs_core::{Module, TypeConstructor, TypeId};
use psrs_hir::{TypeId as HirTypeId, TypeVariableId};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewtypeCallable {
    /// The outer declaration's complete field, including nested applications.
    pub source_field: TypeId,
    pub source_variables: Vec<TypeVariableId>,
    /// The declaration that owns the final canonical callable field.
    pub storage_owner: HirTypeId,
    pub storage_field: TypeId,
    pub parameters: Vec<TypeId>,
    /// Preserve structural results and returned closures as actual type ids.
    pub result: TypeId,
}

pub(super) fn callable(module: &Module, id: HirTypeId) -> Option<NewtypeCallable> {
    let outer = constructor(module, id)?;
    let source_field = outer.field_types[0];
    let mut storage_owner = id;
    let mut storage_field = source_field;
    let mut visited = HashSet::from([id]);
    loop {
        if crate::cc::is_callable_type(module, storage_field) {
            let (parameters, result) =
                crate::cc::checked_function_arrow_parameters(module, storage_field).ok()?;
            return Some(NewtypeCallable {
                source_field,
                source_variables: outer.parameters.clone(),
                storage_owner,
                storage_field,
                parameters,
                result,
            });
        }
        let (_, body) = psrs_core::scheme_parts(&module.types, storage_field)?;
        let (TypeConstructor::User(inner), _) = module.applied_constructor(body)? else {
            return None;
        };
        if !module.newtype_ids.contains(&inner) || !visited.insert(inner) {
            return None;
        }
        storage_owner = inner;
        storage_field = constructor(module, inner)?.field_types[0];
    }
}

fn constructor(module: &Module, id: HirTypeId) -> Option<&psrs_core::ConstructorInfo> {
    let mut constructors = module.constructors.iter().filter(|ctor| ctor.type_id == id);
    let constructor = constructors.next()?;
    (constructors.next().is_none()
        && constructor.field_count == 1
        && constructor.field_types.len() == 1)
        .then_some(constructor)
}

#[cfg(test)]
mod tests;
