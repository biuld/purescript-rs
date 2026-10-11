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
        BackendError::invalid_ir("P9 runtime projection", binding.span, message)
            .with_module(binding.source_module),
    ]
}

/// Plan raw types in the application's own layout arena. This does not box
/// operands, recover results, emit calls, or authorize dependency erasure.
///
/// The source operand projection depended on the withdrawn State protocol, so
/// no generic replacement exists. This reports the missing interface explicitly
/// instead of planning a call from an erased contract.
pub(crate) fn plan_import(
    binding: &RuntimeBinding,
    _logical: &cc::Module,
    _layout: &PlannedLayout,
) -> Result<Import, Vec<BackendError>> {
    Err(error(
        binding,
        "runtime storage bindings require the withdrawn State contract",
    ))
}
