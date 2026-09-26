//! Target-aware linking helpers for source WIT bindings.
//!
//! A foreign import's resolved source type is interned into the Core type table
//! so the backend can refer to it by identity. A structurally equal Core type
//! already in the table is reused, so a foreign signature shares the canonical
//! representation of the same type used elsewhere in the module. This replaces
//! recovering the type by structural search at the CC boundary.

use super::flatten::{self, FlatSlot};
use super::validation::enum_cases;
use super::{WasiField, WasiImport, WasiParamKind, WasiResultKind, source_field_name};
use crate::types::ValueType;
use psrs_core::{
    ConstructorInfo, Module as CoreModule, Type as CoreType, TypeConstructor, TypeId as CoreTypeId,
};
use psrs_hir::{BuiltinType, Type as HirType, TypeKind as HirTypeKind};

/// Interns the resolved source type of a foreign import and returns its
/// [`CoreTypeId`]. The type is appended to the module type table when no
/// structurally equal type is present.
pub(crate) fn intern_source_type(module: &mut CoreModule, ty: &HirType) -> Option<CoreTypeId> {
    intern_into(&mut module.types, &module.constructors, ty)
}

fn intern_into(
    types: &mut Vec<CoreType>,
    constructors: &[ConstructorInfo],
    ty: &HirType,
) -> Option<CoreTypeId> {
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
        HirTypeKind::Named(type_id) => {
            enum_cases(constructors, *type_id)?;
            CoreType::Constructor(TypeConstructor::User(*type_id))
        }
        HirTypeKind::Application(function, argument) => {
            let function = intern_into(types, constructors, function)?;
            let argument = intern_into(types, constructors, argument)?;
            if is_array_constructor(types, function) && !is_array_element(types, argument) {
                return None;
            }
            CoreType::Application(function, argument)
        }
        HirTypeKind::Function { parameter, result } => {
            let parameter = intern_into(types, constructors, parameter)?;
            let result = intern_into(types, constructors, result)?;
            CoreType::Function { parameter, result }
        }
        HirTypeKind::Record { fields, tail: None } => {
            let mut fields = fields
                .iter()
                .map(|field| {
                    let id = intern_into(types, constructors, &field.ty)?;
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

/// Validates a resolved WIT descriptor against a foreign import's resolved Core
/// type. Record field names and enum case order are read from Core, so this
/// runs where Core is available and needs no source-type mirror.
pub(crate) fn validate_import_signature(
    import: &WasiImport,
    module: &CoreModule,
    type_id: CoreTypeId,
) -> Result<(), String> {
    let Some((parameters, result)) = function_parts(module, type_id) else {
        return Err(incompatible(import));
    };
    let all_primitive =
        parameters.iter().all(|id| is_primitive(module, *id)) && is_primitive(module, result);
    if all_primitive && parameters.len() != import.param_kinds.len() {
        validate_primitive_flattening(import, module, &parameters)?;
    } else {
        validate_zipped_parameters(import, module, &parameters)?;
    }
    if core_matches_result(module, result, import) {
        Ok(())
    } else {
        Err(format!(
            "WIT import `{}` has a source result type incompatible with its canonical result",
            import.name
        ))
    }
}

fn incompatible(import: &WasiImport) -> String {
    format!(
        "WIT import `{}` has a source type incompatible with its canonical signature",
        import.name
    )
}

fn mismatch(import: &WasiImport) -> String {
    format!(
        "WIT import `{}` expects {} source arguments, but its declaration has a different arity",
        import.name,
        import.param_kinds.len()
    )
}

/// Walks a Core function type into its parameter types and final result type.
pub(crate) fn function_parts(
    module: &CoreModule,
    type_id: CoreTypeId,
) -> Option<(Vec<CoreTypeId>, CoreTypeId)> {
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

fn core(module: &CoreModule, id: CoreTypeId) -> Option<&CoreType> {
    module.types.get(id.0 as usize)
}

fn is_primitive(module: &CoreModule, id: CoreTypeId) -> bool {
    matches!(
        core(module, id),
        Some(
            CoreType::I32
                | CoreType::Boolean
                | CoreType::F64
                | CoreType::Char
                | CoreType::String
                | CoreType::Unit
        )
    )
}

fn validate_primitive_flattening(
    import: &WasiImport,
    module: &CoreModule,
    parameters: &[CoreTypeId],
) -> Result<(), String> {
    if parameters
        .iter()
        .any(|id| matches!(core(module, *id), Some(CoreType::Unit)))
    {
        return Err(format!(
            "WIT import `{}` uses Unit as a parameter, but Unit is not a canonical parameter",
            import.name
        ));
    }
    let direct = import
        .parameters
        .len()
        .saturating_sub(usize::from(import.retptr));
    let slots_match_core = import.flat_slots.len() == direct
        && import
            .flat_slots
            .iter()
            .zip(&import.parameters)
            .all(|(slot, parameter)| flatten::slot_value_type(slot) == Some(*parameter));
    if !slots_match_core || !parameters_match(module, parameters, &import.flat_slots) {
        return Err(format!(
            "WIT import `{}` primitive flattening does not match the canonical signature",
            import.name
        ));
    }
    Ok(())
}

fn validate_zipped_parameters(
    import: &WasiImport,
    module: &CoreModule,
    parameters: &[CoreTypeId],
) -> Result<(), String> {
    if parameters.len() != import.param_kinds.len() {
        return Err(mismatch(import));
    }
    for (id, kind) in parameters.iter().zip(&import.param_kinds) {
        if !core_matches_kind(module, *id, kind) {
            return Err(format!(
                "WIT import `{}` has a source parameter with an incompatible type",
                import.name
            ));
        }
    }
    Ok(())
}

fn core_matches_result(module: &CoreModule, result: CoreTypeId, import: &WasiImport) -> bool {
    match &import.result_kind {
        WasiResultKind::None => matches!(core(module, result), Some(CoreType::Unit)),
        WasiResultKind::Scalar => match import.result {
            Some(ValueType::I64 | ValueType::I32) => {
                matches!(core(module, result), Some(CoreType::I32))
            }
            Some(ValueType::F32 | ValueType::F64) => {
                matches!(core(module, result), Some(CoreType::F64))
            }
            _ => false,
        },
        WasiResultKind::Handle(_) => core_is_handle(module, result),
        WasiResultKind::IntegerNarrow { .. } => matches!(core(module, result), Some(CoreType::I32)),
        WasiResultKind::Boolean => matches!(core(module, result), Some(CoreType::Boolean)),
        WasiResultKind::Enum { cases } => core_enum_cases(module, result).as_ref() == Some(cases),
        WasiResultKind::Char => matches!(core(module, result), Some(CoreType::Char)),
        WasiResultKind::List => matches!(core(module, result), Some(CoreType::String)),
        WasiResultKind::ValueList { element } => match core(module, result) {
            Some(CoreType::Application(function, argument)) if is_array(module, *function) => {
                core_matches_kind(module, *argument, element)
            }
            _ => false,
        },
        WasiResultKind::Result => matches!(core(module, result), Some(CoreType::Unit)),
        WasiResultKind::Discarded => false,
    }
}

fn core_matches_kind(module: &CoreModule, id: CoreTypeId, kind: &WasiParamKind) -> bool {
    match kind {
        WasiParamKind::Integer32
        | WasiParamKind::IntegerNarrow { .. }
        | WasiParamKind::Scalar64 { .. } => matches!(core(module, id), Some(CoreType::I32)),
        WasiParamKind::Boolean => matches!(core(module, id), Some(CoreType::Boolean)),
        WasiParamKind::Char => matches!(core(module, id), Some(CoreType::Char)),
        WasiParamKind::Float32 | WasiParamKind::Float64 => {
            matches!(core(module, id), Some(CoreType::F64))
        }
        WasiParamKind::Enum { cases } => core_enum_cases(module, id).as_ref() == Some(cases),
        WasiParamKind::Flags { names } => core_matches_flags(module, id, names),
        WasiParamKind::Record { fields } => core_matches_record(module, id, fields),
        WasiParamKind::Handle(_) => core_is_handle(module, id),
        WasiParamKind::List => matches!(core(module, id), Some(CoreType::String)),
        WasiParamKind::ValueList { element } => match core(module, id) {
            Some(CoreType::Application(function, argument)) if is_array(module, *function) => {
                core_matches_kind(module, *argument, element)
            }
            _ => false,
        },
        WasiParamKind::Unsupported => false,
    }
}

fn core_is_handle(module: &CoreModule, id: CoreTypeId) -> bool {
    match core(module, id) {
        Some(CoreType::I32) => true,
        Some(CoreType::Constructor(TypeConstructor::User(type_id))) => {
            module.opaque_ids.contains(type_id)
        }
        _ => false,
    }
}

fn core_enum_cases(module: &CoreModule, id: CoreTypeId) -> Option<Vec<String>> {
    match core(module, id)? {
        CoreType::Constructor(TypeConstructor::User(type_id)) => {
            enum_cases(&module.constructors, *type_id)
        }
        _ => None,
    }
}

fn core_matches_flags(module: &CoreModule, id: CoreTypeId, names: &[String]) -> bool {
    let Some(CoreType::Record(fields)) = core(module, id) else {
        return false;
    };
    names.len() == fields.len()
        && names.iter().all(|name| {
            let source_name = source_field_name(name);
            fields.iter().any(|(label, field)| {
                label == &source_name && matches!(core(module, *field), Some(CoreType::Boolean))
            })
        })
}

fn core_matches_record(module: &CoreModule, id: CoreTypeId, fields: &[WasiField]) -> bool {
    let Some(CoreType::Record(source_fields)) = core(module, id) else {
        return false;
    };
    if fields.len() != source_fields.len() {
        return false;
    }
    fields.iter().all(|field| {
        source_fields
            .iter()
            .find(|(label, _)| source_field_name(&field.name) == *label)
            .is_some_and(|(_, source)| core_matches_kind(module, *source, &field.kind))
    })
}

fn is_array(module: &CoreModule, id: CoreTypeId) -> bool {
    matches!(
        core(module, id),
        Some(CoreType::Constructor(TypeConstructor::Array))
    )
}

/// Walks `parameters` against `slots`. `String` consumes `(pointer, length)`.
/// `Int` matches an integer slot or a handle, not a `Char` or a `Boolean`.
fn parameters_match(module: &CoreModule, parameters: &[CoreTypeId], slots: &[FlatSlot]) -> bool {
    let mut index = 0;
    for parameter in parameters {
        if !consume(module, *parameter, slots, &mut index) {
            return false;
        }
    }
    index == slots.len()
}

fn consume(module: &CoreModule, id: CoreTypeId, slots: &[FlatSlot], index: &mut usize) -> bool {
    let Some(slot) = slots.get(*index) else {
        return false;
    };
    match core(module, id) {
        Some(CoreType::I32) => matches!(
            slot,
            FlatSlot::Int32 | FlatSlot::Int64 { .. } | FlatSlot::Handle
        ),
        Some(CoreType::Boolean) => matches!(slot, FlatSlot::Boolean),
        Some(CoreType::Char) => matches!(slot, FlatSlot::Char),
        Some(CoreType::F64) => matches!(slot, FlatSlot::Float32 | FlatSlot::Float64),
        Some(CoreType::String) => {
            let length = slots.get(*index + 1);
            if matches!((slot, length), (FlatSlot::Pointer, Some(FlatSlot::Length))) {
                *index += 2;
                return true;
            }
            return false;
        }
        _ => return false,
    }
    .then(|| {
        *index += 1;
    })
    .is_some()
}

#[cfg(test)]
mod tests;
