//! WIT conformance validation against a foreign import's resolved Core type.
//!
//! Record field names, enum case order, and the `Maybe`/`Either`/variant
//! mappings are read from Core, so this runs where Core is available and needs
//! no source-type mirror (DEC-12, DEC-13).

use super::super::flatten::{self, FlatSlot};
use super::super::validation::enum_cases;
use super::super::{
    WasiField, WasiImport, WasiParamKind, WasiResultKind, WasiVariantCase, source_field_name,
};
use crate::types::ValueType;
use psrs_core::{
    ConstructorInfo, Module as CoreModule, Type as CoreType, TypeConstructor, TypeId as CoreTypeId,
};
use psrs_hir::TypeId as HirTypeId;

/// Validates a resolved WIT descriptor against a foreign import's resolved Core
/// type.
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
        WasiResultKind::Option { payload } => core_matches_option(module, result, payload),
        WasiResultKind::ValueResult { ok, err } => core_matches_either(module, result, ok, err),
        WasiResultKind::Variant { cases } => core_matches_variant(module, result, cases),
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
        WasiParamKind::Option { payload } => core_matches_option(module, id, payload),
        WasiParamKind::Result { ok, err } => core_matches_either(module, id, ok, err),
        WasiParamKind::Variant { cases } => core_matches_variant(module, id, cases),
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

/// The qualified name of the declaration head of a Core type, following
/// applications to the constructor. `None` when the head is not a known named
/// declaration.
fn user_type_name(module: &CoreModule, id: CoreTypeId) -> Option<&str> {
    let (hir, _) = applied_parts(module, id);
    let hir = hir?;
    module
        .type_names
        .iter()
        .find(|(candidate, _)| *candidate == hir)
        .map(|(_, name)| name.as_str())
}

/// The head constructor and its type arguments, in application order.
fn applied_parts(module: &CoreModule, id: CoreTypeId) -> (Option<HirTypeId>, Vec<CoreTypeId>) {
    let mut arguments = Vec::new();
    let mut current = id;
    while let Some(CoreType::Application(function, argument)) = core(module, current) {
        arguments.push(*argument);
        current = *function;
    }
    arguments.reverse();
    (head_type_id(module, current), arguments)
}

fn head_type_id(module: &CoreModule, id: CoreTypeId) -> Option<HirTypeId> {
    match core(module, id)? {
        CoreType::Constructor(TypeConstructor::User(hir)) => Some(*hir),
        _ => None,
    }
}

/// The constructors of a user type in tag order, each with its field count.
fn core_constructors(module: &CoreModule, hir: HirTypeId) -> Vec<&ConstructorInfo> {
    let mut constructors = module
        .constructors
        .iter()
        .filter(|constructor| constructor.type_id == hir)
        .collect::<Vec<_>>();
    constructors.sort_by_key(|constructor| constructor.tag);
    constructors
}

/// WIT `option<T>` recognized as `Data.Maybe.Maybe T`: two constructors in
/// order, `Nothing` nullary and `Just` with one field matching `payload`.
fn core_matches_option(module: &CoreModule, id: CoreTypeId, payload: &WasiParamKind) -> bool {
    let (hir, arguments) = applied_parts(module, id);
    let Some(hir) = hir else {
        return false;
    };
    if user_type_name(module, id) != Some("Data.Maybe.Maybe") || arguments.len() != 1 {
        return false;
    }
    let constructors = core_constructors(module, hir);
    first_constructor_shape(&constructors) == Some((0, 1))
        && core_matches_kind(module, arguments[0], payload)
}

/// WIT `result<O, E>` recognized as `Data.Either.Either O E`: two constructors
/// in order, `Left` matching `ok` and `Right` matching `err`.
fn core_matches_either(
    module: &CoreModule,
    id: CoreTypeId,
    ok: &WasiParamKind,
    err: &WasiParamKind,
) -> bool {
    let (hir, arguments) = applied_parts(module, id);
    let Some(hir) = hir else {
        return false;
    };
    if user_type_name(module, id) != Some("Data.Either.Either") || arguments.len() != 2 {
        return false;
    }
    let constructors = core_constructors(module, hir);
    core_variant_shape(&constructors, &[Some(ok), Some(err)])
        && core_matches_kind(module, arguments[0], ok)
        && core_matches_kind(module, arguments[1], err)
}

/// WIT `variant { ... }` recognized as a source data type whose constructors
/// follow the WIT case order, with each payload matching by field type.
fn core_matches_variant(module: &CoreModule, id: CoreTypeId, cases: &[WasiVariantCase]) -> bool {
    let (hir, arguments) = applied_parts(module, id);
    let Some(hir) = hir else {
        return false;
    };
    if !arguments.is_empty() {
        return false;
    }
    let constructors = core_constructors(module, hir);
    if !core_variant_shape(
        &constructors,
        &cases
            .iter()
            .map(|case| case.kind.as_deref())
            .collect::<Vec<_>>(),
    ) {
        return false;
    }
    cases
        .iter()
        .zip(&constructors)
        .all(|(case, constructor)| match &case.kind {
            None => true,
            Some(kind) => constructor
                .field_types
                .first()
                .is_some_and(|field| core_matches_kind(module, *field, kind)),
        })
}

/// The `(field_count, tag)` shape of a two-constructor type: tag 0 nullary and
/// tag 1 with one field.
fn first_constructor_shape(constructors: &[&ConstructorInfo]) -> Option<(u32, u32)> {
    match constructors {
        [first, second] if first.tag == 0 && second.tag == 1 => {
            Some((first.field_count as u32, second.field_count as u32))
        }
        _ => None,
    }
}

/// Whether the constructors are in tag order and the payload presence matches
/// each case: a nullary case has no field, a payload case has exactly one.
fn core_variant_shape(constructors: &[&ConstructorInfo], cases: &[Option<&WasiParamKind>]) -> bool {
    constructors.len() == cases.len()
        && constructors
            .iter()
            .zip(cases)
            .enumerate()
            .all(|(tag, (constructor, case))| {
                constructor.tag == tag as u32
                    && match case {
                        None => constructor.field_count == 0,
                        Some(_) => constructor.field_count == 1,
                    }
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
