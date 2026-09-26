use super::{SourceType, WasiParamKind, source_field_name};
use crate::TargetCapabilities;

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
        WasiParamKind::Scalar64 { .. } | WasiParamKind::Handle => {
            matches!(source, SourceType::Int)
        }
        WasiParamKind::List => matches!(source, SourceType::String),
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
        WasiParamKind::List => 2,
        WasiParamKind::Record { fields } => fields
            .iter()
            .map(|field| flattened_parameter_count(&field.kind))
            .sum(),
        WasiParamKind::Flags { names } => names.len().div_ceil(32),
        WasiParamKind::Unsupported => 0,
        _ => 1,
    }
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
