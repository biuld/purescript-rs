//! Source-ABI surface checks over resolved canonical types.

use super::canonical::{
    CanonicalType, contains_rejected_list, parameter_has_source_abi, primitive_aggregate_allowed,
};
use crate::TargetCapabilities;
use psrs_core::ConstructorInfo;
use psrs_hir::TypeId as HirTypeId;

/// The first reason a WIT function is outside the source ABI subset, in check
/// order. A parameter with no canonical form (`map`, `future`, `stream`, a raw
/// resource, `error-context`, or an unknown definition) counts as unsupported.
pub(super) fn unsupported_shape(
    params: &[Option<CanonicalType>],
    result_shape: &Result<Option<CanonicalType>, ()>,
    result: &Option<CanonicalType>,
) -> Option<String> {
    if params
        .iter()
        .flatten()
        .any(|ty| contains_rejected_list(ty, false))
    {
        return Some("non-byte WIT lists are not supported by the String ABI".into());
    }
    // `option`, tuple, and payload-bearing variant need the primitive check
    // against canonical flat leaves, so this pass only rejects shapes with no
    // source ABI mapping at all.
    if params.iter().any(|parameter| match parameter {
        Some(ty) => !parameter_has_source_abi(ty) && !primitive_aggregate_allowed(ty),
        None => true,
    }) {
        return Some("WIT parameter shape has no source ABI mapping yet".into());
    }
    if let Some(ty) = result
        && contains_rejected_list(ty, false)
        && !ty.is_byte_list()
    {
        return Some("non-byte WIT list results are not supported by the String ABI".into());
    }
    if result_present_not_supported(result_shape) {
        return Some("WIT result shape has no source ABI mapping yet".into());
    }
    None
}

/// Whether a function declares a result that the source ABI cannot represent.
/// An unresolvable result counts as unsupported.
fn result_present_not_supported(result_shape: &Result<Option<CanonicalType>, ()>) -> bool {
    match result_shape {
        Err(()) => true,
        Ok(None) => false,
        Ok(Some(ty)) => !result_has_source_abi(ty),
    }
}

/// Whether a canonical result has a source-ABI result lowering.
fn result_has_source_abi(ty: &CanonicalType) -> bool {
    match ty {
        CanonicalType::Bool
        | CanonicalType::Int { .. }
        | CanonicalType::Float { .. }
        | CanonicalType::Char
        | CanonicalType::Enum(_)
        | CanonicalType::Handle { .. }
        | CanonicalType::String => true,
        CanonicalType::List(element) => element.is_byte() || list_element_supported(element),
        CanonicalType::FixedList { element, .. } => element.is_byte(),
        CanonicalType::Option(payload) => direct(payload),
        CanonicalType::Result { ok, err } => match (ok.as_deref(), err.as_deref()) {
            (Some(ok), Some(err)) => direct(ok) && direct(err),
            (None, _) => true,
            (Some(_), None) => false,
        },
        CanonicalType::Variant(cases) => cases
            .iter()
            .all(|case| case.payload.as_deref().is_none_or(direct)),
        CanonicalType::Record(_) | CanonicalType::Flags(_) => false,
    }
}

/// Whether a non-byte list result's element has a source lowering.
fn list_element_supported(element: &CanonicalType) -> bool {
    match element {
        CanonicalType::String => true,
        CanonicalType::List(inner) => inner.is_byte(),
        CanonicalType::FixedList { element, .. } => element.is_byte(),
        CanonicalType::Bool
        | CanonicalType::Int { .. }
        | CanonicalType::Float { .. }
        | CanonicalType::Char
        | CanonicalType::Enum(_)
        | CanonicalType::Flags(_)
        | CanonicalType::Handle { .. } => true,
        CanonicalType::Record(fields) => fields.iter().all(|field| direct(&field.ty)),
        _ => false,
    }
}

/// Whether a payload can be lowered directly inside an aggregate.
fn direct(ty: &CanonicalType) -> bool {
    match ty {
        CanonicalType::Bool
        | CanonicalType::Int { .. }
        | CanonicalType::Float { .. }
        | CanonicalType::Char
        | CanonicalType::String
        | CanonicalType::Enum(_)
        | CanonicalType::Flags(_)
        | CanonicalType::Handle { .. }
        | CanonicalType::List(_) => true,
        CanonicalType::FixedList { element, .. } => element.is_byte(),
        CanonicalType::Record(fields) => fields.iter().all(|field| direct(&field.ty)),
        CanonicalType::Option(payload) => direct(payload),
        CanonicalType::Result { ok, err } => match (ok.as_deref(), err.as_deref()) {
            (Some(ok), Some(err)) => direct(ok) && direct(err),
            _ => false,
        },
        CanonicalType::Variant(cases) => cases
            .iter()
            .all(|case| case.payload.as_deref().is_none_or(direct)),
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
