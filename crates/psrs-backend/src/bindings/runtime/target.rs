//! Storage-provider ABI adaptation at the external-binding boundary.
//! MIR consumers receive physical signatures and a general return convention.
use crate::mir::{Import, RuntimeImport, layout::PlannedLayout};
use crate::types::{CompositeType, FieldType, HeapType, RecGroup, RefType, StorageType, ValueType};
use crate::{BackendError, RuntimeBinding, cc};
use psrs_runtime::{StorageResultProjection, StorageType as RawType};

/// Result adaptation for a checked external call, independent of its provider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RawCallResult {
    Value,
    Unit,
    Never,
}

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
pub(crate) fn plan_import(
    binding: &RuntimeBinding,
    logical: &cc::Module,
    layout: &PlannedLayout,
) -> Result<(Import, RawCallResult), Vec<BackendError>> {
    let raw = crate::bindings::checked_runtime_call(binding, logical)?;
    let signature = logical
        .externals
        .iter()
        .find(|external| external.symbol == binding.symbol)
        .and_then(|external| external.signature.as_ref())
        .unwrap();
    let state = cc::state::StateCallProjection::checked(signature, &logical.representations)
        .map_err(|message| error(binding, message))?
        .unwrap();
    let physical = |raw: RawType, shape: cc::ValueShape| -> Result<ValueType, Vec<BackendError>> {
        match raw {
            RawType::I32 => Ok(ValueType::I32),
            RawType::Value => Ok(ValueType::Ref(RefType {
                nullable: true,
                heap: HeapType::Eq,
            })),
            RawType::Array => {
                let ty = layout.value_type(&shape).map_err(|_| {
                    error(
                        binding,
                        "runtime array has no planned type in the application arena",
                    )
                })?;
                let ValueType::Ref(RefType {
                    nullable: false,
                    heap: HeapType::Index(_),
                }) = ty
                else {
                    return Err(error(
                        binding,
                        "runtime array has no non-null concrete target type",
                    ));
                };
                if !matches_raw_type(raw, ty, layout.types()) {
                    return Err(error(
                        binding,
                        "runtime array target must be mutable nullable-eqref storage",
                    ));
                }
                Ok(ty)
            }
        }
    };
    let parameters = raw
        .operands()
        .zip(&signature.parameters)
        .map(|((_, raw), shape)| physical(raw, *shape))
        .collect::<Result<Vec<_>, _>>()?;
    let result = raw
        .abi()
        .result
        .map(|raw| physical(raw, state.payload))
        .transpose()?;
    Ok((
        Import {
            runtime: Some(RuntimeImport {
                module: binding.module.clone(),
                function: binding.function.clone(),
            }),
            symbol: binding.symbol,
            parameters,
            result,
        },
        match raw.result() {
            StorageResultProjection::Value(_) => RawCallResult::Value,
            StorageResultProjection::Unit => RawCallResult::Unit,
            StorageResultProjection::Never => RawCallResult::Never,
        },
    ))
}
