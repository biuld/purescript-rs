//! Target-aware linking helpers for source WIT bindings.
//!
//! WIT declarations consume the checked external schemes carried by Core.
//! Structural HIR interning remains available only to isolated ABI fixtures;
//! production linking never reconstructs the checked source type from HIR.

#[cfg(test)]
use psrs_core::{Module as CoreModule, Type as CoreType, TypeConstructor, TypeId as CoreTypeId};
#[cfg(test)]
use psrs_hir::{BuiltinType, Type as HirType, TypeKind as HirTypeKind};

/// Interns the resolved source type of a foreign import and returns its
/// [`CoreTypeId`]. The type is appended to the module type table when no
/// structurally equal type is present.
#[cfg(test)]
pub(crate) fn intern_source_type(module: &mut CoreModule, ty: &HirType) -> Option<CoreTypeId> {
    intern_into(&mut module.types, ty)
}

#[cfg(test)]
fn intern_into(types: &mut Vec<CoreType>, ty: &HirType) -> Option<CoreTypeId> {
    let core = match &ty.kind {
        HirTypeKind::Constructor(BuiltinType::Int) => {
            CoreType::Constructor(psrs_core::TypeConstructor::Int)
        }
        HirTypeKind::Constructor(BuiltinType::Boolean) => {
            CoreType::Constructor(psrs_core::TypeConstructor::Boolean)
        }
        HirTypeKind::Constructor(BuiltinType::Number) => {
            CoreType::Constructor(psrs_core::TypeConstructor::Number)
        }
        HirTypeKind::Constructor(BuiltinType::Char) => {
            CoreType::Constructor(psrs_core::TypeConstructor::Char)
        }
        HirTypeKind::Constructor(BuiltinType::String) => {
            CoreType::Constructor(psrs_core::TypeConstructor::String)
        }
        HirTypeKind::Constructor(BuiltinType::Unit) => {
            CoreType::Constructor(psrs_core::TypeConstructor::Unit)
        }
        HirTypeKind::Constructor(BuiltinType::Array) => {
            CoreType::Constructor(TypeConstructor::Array)
        }
        HirTypeKind::Opaque(type_id) => CoreType::Constructor(TypeConstructor::User(*type_id)),
        // A named data type keeps its identity. The nullary-enum, `Maybe`,
        // `Either`, and variant mappings are all validated later against the
        // WIT descriptor, so the interner must not reject payload-bearing
        // declarations (DEC-13).
        HirTypeKind::Named(type_id) => CoreType::Constructor(TypeConstructor::User(*type_id)),
        HirTypeKind::Application(function, argument) => {
            let function = intern_into(types, function)?;
            let argument = intern_into(types, argument)?;
            if is_array_constructor(types, function) && !is_array_element(types, argument) {
                return None;
            }
            CoreType::Application(function, argument)
        }
        HirTypeKind::Function { parameter, result } => {
            let parameter = intern_into(types, parameter)?;
            let result = intern_into(types, result)?;
            let head = intern_core_type(types, CoreType::Constructor(TypeConstructor::Function));
            let inner = intern_core_type(types, CoreType::Application(head, parameter));
            return Some(intern_core_type(
                types,
                CoreType::Application(inner, result),
            ));
        }
        HirTypeKind::Record { fields, tail: None } => {
            let mut fields = fields
                .iter()
                .map(|field| {
                    let id = intern_into(types, &field.ty)?;
                    Some((field.label.clone(), id))
                })
                .collect::<Option<Vec<_>>>()?;
            fields.sort_by(|left, right| left.0.cmp(&right.0));
            let mut row = intern_core_type(types, CoreType::RowEmpty);
            for (label, ty) in fields.into_iter().rev() {
                row = intern_core_type(
                    types,
                    CoreType::RowExtend {
                        label,
                        ty,
                        tail: row,
                    },
                );
            }
            let head = intern_core_type(types, CoreType::Constructor(TypeConstructor::Record));
            return Some(intern_core_type(types, CoreType::Application(head, row)));
        }
        _ => return None,
    };
    Some(intern_core_type(types, core))
}

#[cfg(test)]
fn is_array_constructor(types: &[CoreType], id: CoreTypeId) -> bool {
    matches!(
        types.get(id.0 as usize),
        Some(CoreType::Constructor(TypeConstructor::Array))
    )
}

#[cfg(test)]
fn is_array_element(types: &[CoreType], id: CoreTypeId) -> bool {
    match types.get(id.0 as usize) {
        // `Unit` is a primitive but has no canonical list element.
        Some(CoreType::Constructor(constructor))
            if constructor.is_primitive() && *constructor != TypeConstructor::Unit =>
        {
            true
        }
        // A nullary enum or an opaque handle. The interner only admits `Named`
        // for a nullary enum, so a field-bearing type never reaches here.
        Some(CoreType::Constructor(TypeConstructor::User(_))) => true,
        // A closed record has its own representation.
        Some(_) if is_record(types, id) => true,
        // A nested `Array T` element lowers recursively; an applied user type
        // is either a newtype such as `Resource a` or a parameterized data type
        // (`Maybe a`, `Either e a`), whose element conformance is validated
        // against the canonical WIT element.
        Some(CoreType::Application(function, _)) => {
            is_array_constructor(types, *function) || is_user_type(types, *function)
        }
        _ => false,
    }
}

#[cfg(test)]
fn is_record(types: &[CoreType], id: CoreTypeId) -> bool {
    let Some(CoreType::Application(function, _)) = types.get(id.0 as usize) else {
        return false;
    };
    matches!(
        types.get(function.0 as usize),
        Some(CoreType::Constructor(TypeConstructor::Record))
    )
}

#[cfg(test)]
fn is_user_type(types: &[CoreType], id: CoreTypeId) -> bool {
    matches!(
        types.get(id.0 as usize),
        Some(CoreType::Constructor(TypeConstructor::User(_)))
    )
}

#[cfg(test)]
fn intern_core_type(types: &mut Vec<CoreType>, core: CoreType) -> CoreTypeId {
    if let Some(index) = types.iter().position(|existing| *existing == core) {
        return CoreTypeId(index as u32);
    }
    types.push(core);
    CoreTypeId((types.len() - 1) as u32)
}

mod conformance;
mod function_type;

pub(crate) use conformance::validate_import_signature;
pub(crate) use function_type::function_parts;

#[cfg(test)]
mod tests;
