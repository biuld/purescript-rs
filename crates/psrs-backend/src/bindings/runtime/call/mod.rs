//! Checked logical CC operands for the runtime-owned raw storage contract.
use crate::BackendError;
use crate::bindings::RuntimeBinding;
use crate::cc::{self, RefShape, Reference, Representation, ValueShape};
use psrs_runtime::{StorageCallProjection, StorageOperation, StorageValue};

/// Source element equality and nominal regions are checked against Core before
/// CC publication. This boundary checks the retained logical slots and storage
/// layout; it does not authorize State erasure or prove instruction ordering.
pub(crate) fn checked(
    binding: &RuntimeBinding,
    module: &cc::Module,
) -> Result<StorageCallProjection, Vec<BackendError>> {
    let error = |message: &str| {
        vec![
            BackendError::invalid_ir("P9 runtime projection", binding.span, message)
                .with_module(binding.source_module),
        ]
    };
    let external = module
        .externals
        .iter()
        .find(|external| external.symbol == binding.symbol)
        .ok_or_else(|| error("runtime CC binding has no external declaration"))?;
    let signature = external
        .signature
        .as_ref()
        .ok_or_else(|| error("runtime CC binding has no logical signature"))?;
    checked_signature(binding, signature, &module.representations)
}

fn checked_signature(
    binding: &RuntimeBinding,
    signature: &cc::Signature,
    table: &cc::RepresentationTable,
) -> Result<StorageCallProjection, Vec<BackendError>> {
    let error = |message: &str| {
        vec![
            BackendError::invalid_ir("P9 runtime projection", binding.span, message)
                .with_module(binding.source_module),
        ]
    };
    if binding.module != psrs_runtime::STORAGE_MODULE {
        return Err(error("runtime CC binding has an unknown provider"));
    }
    let operation = StorageOperation::from_export(&binding.function)
        .ok_or_else(|| error("runtime CC binding has an unknown export"))?;
    let raw = operation.projection().map_err(error)?;
    let state = cc::state::StateCallProjection::checked(signature, table)
        .map_err(error)?
        .ok_or_else(|| error("runtime CC binding requires a checked State-to-Step signature"))?;
    if state.state_parameter != raw.operands().len() {
        return Err(error("runtime CC and raw operand counts disagree"));
    }
    for (shape, role) in signature.parameters[..state.state_parameter]
        .iter()
        .copied()
        .zip(raw.operands().map(|(source, _)| source))
        .chain([(state.payload, raw.source().payload)])
    {
        match role {
            StorageValue::Int | StorageValue::Unit if shape != ValueShape::Integer => {
                return Err(error(
                    "runtime CC index, length or Unit payload must be Integer",
                ));
            }
            StorageValue::Array => {
                let ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(id),
                }) = shape
                else {
                    return Err(error(
                        "runtime CC array requires a non-null concrete storage reference",
                    ));
                };
                if !matches!(table.representation(id), Some(Representation::Array { element })
                    if *element == cc::payload::erased_shape())
                {
                    return Err(error(
                        "runtime CC array requires canonical erased-element storage",
                    ));
                }
            }
            StorageValue::Element | StorageValue::Any if shape == ValueShape::State => {
                return Err(error(
                    "runtime CC payload cannot materialize a State dependency",
                ));
            }
            _ => {}
        }
    }
    Ok(raw)
}

/// Select the runtime-owned storage roles only after source checking. Ordinary
/// CC conversions adapt checked use types to this protocol before MIR erasure.
pub(crate) fn canonical_signature(
    binding: &RuntimeBinding,
    signature: &cc::Signature,
    table: &cc::RepresentationTable,
) -> Result<cc::Signature, Vec<BackendError>> {
    let projection = checked_signature(binding, signature, table)?;
    let mut result = signature.clone();
    for (shape, (role, _)) in result.parameters.iter_mut().zip(projection.operands()) {
        if role == StorageValue::Element {
            *shape = cc::payload::erased_shape();
        }
    }
    if projection.source().payload == StorageValue::Element {
        let state = cc::state::StateCallProjection::checked(signature, table)
            .map_err(|message| {
                vec![BackendError::invalid_ir(
                    "P8 runtime protocol",
                    binding.span,
                    message,
                )]
            })?
            .expect("checked runtime signature retains State");
        let slot = state
            .payload_slot_signature(signature, table)
            .map_err(|message| {
                vec![BackendError::invalid_ir(
                    "P8 runtime protocol",
                    binding.span,
                    message,
                )]
            })?;
        result.result = table
            .signature(slot)
            .expect("checked payload slot exists")
            .result;
    }
    checked_signature(binding, &result, table)?;
    Ok(result)
}

#[cfg(test)]
mod tests;
