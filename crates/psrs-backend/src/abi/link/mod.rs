//! Target-aware linking helpers for source WIT bindings.
//!
//! A foreign import's resolved source type is interned into the Core type table
//! so the backend can refer to it by identity. A structurally equal Core type
//! already in the table is reused, so a foreign signature shares the canonical
//! representation of the same type used elsewhere in the module. This replaces
//! recovering the type by structural search at the CC boundary.

use psrs_core::{Module as CoreModule, Type as CoreType, TypeConstructor, TypeId as CoreTypeId};
use psrs_hir::{BuiltinType, Type as HirType, TypeKind as HirTypeKind};

/// Interns the resolved source type of a foreign import and returns its
/// [`CoreTypeId`]. The type is appended to the module type table when no
/// structurally equal type is present.
pub(crate) fn intern_source_type(module: &mut CoreModule, ty: &HirType) -> Option<CoreTypeId> {
    intern_into(&mut module.types, ty)
}

fn intern_into(types: &mut Vec<CoreType>, ty: &HirType) -> Option<CoreTypeId> {
    let core = match &ty.kind {
        HirTypeKind::Constructor(BuiltinType::Int) => CoreType::I32,
        HirTypeKind::Constructor(BuiltinType::Boolean) => CoreType::Boolean,
        HirTypeKind::Constructor(BuiltinType::Number) => CoreType::F64,
        HirTypeKind::Constructor(BuiltinType::Char) => CoreType::Char,
        HirTypeKind::Constructor(BuiltinType::String) => CoreType::String,
        HirTypeKind::Constructor(BuiltinType::Unit) => CoreType::Unit,
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
            CoreType::Function { parameter, result }
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
            CoreType::Record(fields)
        }
        _ => return None,
    };
    Some(intern_core_type(types, core))
}

fn is_array_constructor(types: &[CoreType], id: CoreTypeId) -> bool {
    matches!(
        types.get(id.0 as usize),
        Some(CoreType::Constructor(TypeConstructor::Array))
    )
}

fn is_array_element(types: &[CoreType], id: CoreTypeId) -> bool {
    matches!(
        types.get(id.0 as usize),
        Some(
            CoreType::I32
                | CoreType::Boolean
                | CoreType::F64
                | CoreType::Char
                | CoreType::String
                | CoreType::Record(_)
                // A nullary enum or an opaque handle. The interner only admits
                // `Named` for a nullary enum, so a field-bearing type never
                // reaches here.
                | CoreType::Constructor(TypeConstructor::User(_))
        )
    )
}

fn intern_core_type(types: &mut Vec<CoreType>, core: CoreType) -> CoreTypeId {
    if let Some(index) = types.iter().position(|existing| *existing == core) {
        return CoreTypeId(index as u32);
    }
    types.push(core);
    CoreTypeId((types.len() - 1) as u32)
}

mod conformance;

pub(crate) use conformance::{function_parts, validate_import_signature};

#[cfg(test)]
mod tests;
