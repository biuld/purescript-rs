//! Storage-provider ABI adaptation at the external-binding boundary.
//! MIR consumers receive physical signatures and a general return convention.
use crate::mir::{Import, layout::PlannedLayout};
use crate::types::{CompositeType, FieldType, HeapType, RecGroup, RefType, StorageType, ValueType};
use crate::{BackendError, RuntimeBinding, cc};
use psrs_runtime::StorageType as RawType;

fn matches_raw_type(raw: RawType, value: ValueType, types: &[RecGroup]) -> bool {
    match (raw, value) {
        (RawType::I32, ValueType::I32) => true,
        (
            RawType::Value,
            ValueType::Ref(RefType {
                nullable: true,
                heap: HeapType::Eq,
            }),
        ) => true,
        (
            RawType::Array,
            ValueType::Ref(RefType {
                nullable: false,
                heap: HeapType::Index(id),
            }),
        ) => {
            let definition = types.iter().flat_map(|group| &group.0).nth(id.0 as usize);
            matches!(
                definition.map(|definition| &definition.composite),
                Some(CompositeType::Array(FieldType {
                    mutable: true,
                    storage: StorageType::Ref(RefType {
                        nullable: true,
                        heap: HeapType::Eq
                    }),
                }))
            )
        }
        _ => false,
    }
}

pub(crate) fn verify_import(import: &Import, types: &[RecGroup]) -> Result<(), String> {
    let Some(provider) = &import.runtime else {
        return Ok(());
    };
    if crate::target_intrinsics::generated::name(import.symbol).is_some()
        || crate::target_runtime::for_symbol(import.symbol).is_some()
    {
        return Err("MIR source runtime import cannot replace a reserved target binding".into());
    }
    if provider.module != psrs_runtime::STORAGE_MODULE {
        return Err("MIR runtime import has an unknown provider".into());
    }
    let operation = psrs_runtime::StorageOperation::from_export(&provider.function)
        .ok_or("MIR runtime import has an unknown export")?;
    let raw = operation.projection().map_err(str::to_owned)?;
    if raw.abi().parameters.len() != import.parameters.len()
        || !raw
            .abi()
            .parameters
            .iter()
            .copied()
            .zip(import.parameters.iter().copied())
            .all(|(raw, value)| matches_raw_type(raw, value, types))
        || match (raw.abi().result, import.result) {
            (Some(raw), Some(value)) => !matches_raw_type(raw, value, types),
            (None, None) => false,
            _ => true,
        }
    {
        return Err("MIR runtime import signature disagrees with its provider contract".into());
    }
    Ok(())
}

pub(crate) fn error(binding: &RuntimeBinding, message: &str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P9 runtime ABI", binding.span, message)
            .with_module(binding.source_module),
    ]
}

/// Plan raw types using the canonical CC signature and the application's arena.
pub(crate) fn plan_import(
    binding: &RuntimeBinding,
    logical: &cc::Module,
    layout: &PlannedLayout,
) -> Result<Import, Vec<BackendError>> {
    super::call::checked(binding, logical)?;
    let signature = logical
        .externals
        .iter()
        .find(|external| external.symbol == binding.symbol)
        .and_then(|external| external.signature.as_ref())
        .ok_or_else(|| error(binding, "runtime external has no CC signature"))?;
    let operation = psrs_runtime::StorageOperation::from_export(&binding.function)
        .ok_or_else(|| error(binding, "unknown runtime storage export"))?;
    let raw = operation
        .projection()
        .map_err(|message| error(binding, message))?;
    let physical = |raw, shape| -> Result<_, Vec<BackendError>> {
        Ok(match raw {
            RawType::I32 => ValueType::I32,
            RawType::Value => ValueType::Ref(RefType {
                nullable: true,
                heap: HeapType::Eq,
            }),
            RawType::Array => layout
                .value_type(&shape)
                .map_err(|_| error(binding, "runtime array has no planned layout"))?,
        })
    };
    let import = Import {
        symbol: binding.symbol,
        runtime: Some(crate::mir::RuntimeImport {
            module: binding.module.clone(),
            function: binding.function.clone(),
        }),
        parameters: raw
            .abi()
            .parameters
            .iter()
            .copied()
            .zip(&signature.parameters)
            .map(|(raw, shape)| physical(raw, *shape))
            .collect::<Result<_, _>>()?,
        result: raw
            .abi()
            .result
            .map(|raw| physical(raw, signature.result))
            .transpose()?,
    };
    verify_import(&import, layout.types()).map_err(|message| error(binding, &message))?;
    Ok(import)
}
