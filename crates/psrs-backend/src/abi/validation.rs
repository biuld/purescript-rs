use super::WasiParamKind;
use crate::TargetCapabilities;
use psrs_core::ConstructorInfo;
use psrs_hir::TypeId as HirTypeId;

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

/// The case names of a nullary source enum, in constructor-tag order. Shared by
/// the Core-based conformance check.
pub(super) fn enum_cases(
    constructors: &[ConstructorInfo],
    type_id: HirTypeId,
) -> Option<Vec<String>> {
    let mut cases = constructors
        .iter()
        .filter(|constructor| constructor.type_id == type_id)
        .collect::<Vec<_>>();
    cases.sort_by_key(|constructor| constructor.tag);
    if cases.is_empty()
        || cases.iter().any(|constructor| constructor.field_count != 0)
        || cases
            .iter()
            .enumerate()
            .any(|(index, constructor)| constructor.tag != index as u32)
    {
        return None;
    }
    Some(
        cases
            .into_iter()
            .map(|constructor| constructor.name.clone())
            .collect(),
    )
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
