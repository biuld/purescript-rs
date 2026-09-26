use super::flatten;
use super::{
    SourceSignature, SourceType, WasiImport, WasiParamKind, WasiResultKind, source_field_name,
};
use crate::TargetCapabilities;
use crate::types::ValueType;

pub(super) fn source_parameter_matches(source: &SourceType, wit: &WasiParamKind) -> bool {
    match wit {
        WasiParamKind::Integer32 => matches!(source, SourceType::Int),
        WasiParamKind::IntegerNarrow { .. } => matches!(source, SourceType::Int),
        WasiParamKind::Boolean => matches!(source, SourceType::Boolean),
        WasiParamKind::Char => matches!(source, SourceType::Char),
        WasiParamKind::Float32 | WasiParamKind::Float64 => matches!(source, SourceType::Number),
        WasiParamKind::Enum { cases } => {
            matches!(source, SourceType::Enum { cases: source } if source == cases)
        }
        WasiParamKind::Flags { names } => {
            let SourceType::Record { fields } = source else {
                return false;
            };
            names.len() == fields.len()
                && names.iter().all(|name| {
                    let source_name = source_field_name(name);
                    fields.iter().any(|(label, ty)| {
                        label == &source_name && matches!(ty.as_ref(), SourceType::Boolean)
                    })
                })
        }
        WasiParamKind::Scalar64 { .. } => matches!(source, SourceType::Int),
        WasiParamKind::Handle(_) => {
            matches!(source, SourceType::Int | SourceType::Resource { .. })
        }
        WasiParamKind::List => matches!(source, SourceType::String),
        WasiParamKind::ValueList { element } => {
            let SourceType::Array { element: source } = source else {
                return false;
            };
            source_parameter_matches(source, element)
        }
        WasiParamKind::Record { fields } => {
            let SourceType::Record {
                fields: source_fields,
            } = source
            else {
                return false;
            };
            if fields.len() != source_fields.len() {
                return false;
            }
            fields.iter().all(|field| {
                source_fields
                    .iter()
                    .find(|(label, _)| source_field_name(&field.name) == *label)
                    .is_some_and(|(_, source)| source_parameter_matches(source, &field.kind))
            })
        }
        WasiParamKind::Unsupported => false,
    }
}

pub(super) fn flattened_parameter_count(kind: &WasiParamKind) -> usize {
    match kind {
        WasiParamKind::List | WasiParamKind::ValueList { .. } => 2,
        WasiParamKind::Record { fields } => fields
            .iter()
            .map(|field| flattened_parameter_count(&field.kind))
            .sum(),
        WasiParamKind::Flags { names } => names.len().div_ceil(32),
        WasiParamKind::Unsupported => 0,
        _ => 1,
    }
}

pub(super) fn validate_import_signature(
    import: &WasiImport,
    signature: &SourceSignature,
) -> Result<(), String> {
    if flatten::is_primitive_signature(&signature.parameters, &signature.result)
        && signature.parameters.len() != import.param_kinds.len()
    {
        validate_primitive_flattening(import, signature)?;
    } else {
        validate_zipped_parameters(import, signature)?;
    }
    validate_result(import, signature)
}

/// A primitive import whose source arity differs from the WIT parameter count.
/// `option<string>` is one WIT parameter and `Int -> String`: a discriminant
/// plus `(pointer, length)`.
fn validate_primitive_flattening(
    import: &WasiImport,
    signature: &SourceSignature,
) -> Result<(), String> {
    if signature
        .parameters
        .iter()
        .any(|parameter| matches!(parameter, SourceType::Unit))
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
    if !slots_match_core || !flatten::parameters_match(&signature.parameters, &import.flat_slots) {
        return Err(format!(
            "WIT import `{}` primitive flattening does not match the canonical signature",
            import.name
        ));
    }
    Ok(())
}

fn validate_zipped_parameters(
    import: &WasiImport,
    signature: &SourceSignature,
) -> Result<(), String> {
    if signature.parameters.len() != import.param_kinds.len() {
        return Err(format!(
            "WIT import `{}` expects {} source arguments, but its declaration has {}",
            import.name,
            import.param_kinds.len(),
            signature.parameters.len()
        ));
    }
    for (parameter, kind) in signature.parameters.iter().zip(&import.param_kinds) {
        if !source_parameter_matches(parameter, kind) {
            return Err(format!(
                "WIT import `{}` has a source parameter with an incompatible type",
                import.name
            ));
        }
    }
    Ok(())
}

fn validate_result(import: &WasiImport, signature: &SourceSignature) -> Result<(), String> {
    let valid_result = match &import.result_kind {
        WasiResultKind::None => matches!(&signature.result, SourceType::Unit),
        WasiResultKind::Scalar => match import.result {
            Some(ValueType::I64) => {
                matches!(&signature.result, SourceType::Int)
            }
            Some(ValueType::I32) => matches!(&signature.result, SourceType::Int),
            Some(ValueType::F32 | ValueType::F64) => {
                matches!(&signature.result, SourceType::Number)
            }
            _ => false,
        },
        WasiResultKind::Handle(_) => {
            matches!(
                &signature.result,
                SourceType::Int | SourceType::Resource { .. }
            )
        }
        WasiResultKind::IntegerNarrow { .. } => matches!(&signature.result, SourceType::Int),
        WasiResultKind::Boolean => matches!(&signature.result, SourceType::Boolean),
        WasiResultKind::Enum { cases } => matches!(
            &signature.result,
            SourceType::Enum { cases: source } if source == cases
        ),
        WasiResultKind::Char => matches!(&signature.result, SourceType::Char),
        WasiResultKind::List => matches!(&signature.result, SourceType::String),
        WasiResultKind::ValueList { element } => match &signature.result {
            SourceType::Array { element: source } => source_parameter_matches(source, element),
            _ => false,
        },
        WasiResultKind::Result => matches!(&signature.result, SourceType::Unit),
        WasiResultKind::Discarded => false,
    };
    if !valid_result {
        return Err(format!(
            "WIT import `{}` has a source result type incompatible with its canonical result",
            import.name
        ));
    }
    Ok(())
}

pub(super) fn wasi_interface_enabled(target: TargetCapabilities, module: &str) -> bool {
    let package_path = module
        .split_once('/')
        .map_or(module, |(package, _)| package);
    let package = package_path
        .split_once('@')
        .map_or(package_path, |(package, _)| package);
    match package {
        "wasi:cli" => target.wasi_cli,
        "wasi:io" => target.wasi_io,
        "wasi:clocks" => target.wasi_clocks,
        "wasi:random" => target.wasi_random,
        "wasi:filesystem" => target.wasi_filesystem,
        "wasi:sockets" => target.wasi_sockets,
        "wasi:http" => target.wasi_http,
        "wasi:tls" => target.wasi_tls,
        _ => false,
    }
}
