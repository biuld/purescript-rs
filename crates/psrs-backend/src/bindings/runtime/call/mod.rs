//! Ordinary CC call signatures for the runtime storage ABI.
use crate::{BackendError, RuntimeBinding, cc};
use psrs_runtime::{StorageOperation, StorageValue};

#[cfg(test)]
mod tests;

fn error(binding: &RuntimeBinding, message: &str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P8 runtime ABI", binding.span, message)
            .with_module(binding.source_module),
    ]
}

pub(crate) fn checked(
    binding: &RuntimeBinding,
    module: &cc::Module,
) -> Result<(), Vec<BackendError>> {
    let signature = module
        .externals
        .iter()
        .find(|external| external.symbol == binding.symbol)
        .and_then(|external| external.signature.as_ref())
        .ok_or_else(|| error(binding, "runtime external has no CC signature"))?;
    let canonical = canonical_signature(binding, signature, &module.representations)?;
    if &canonical != signature {
        return Err(error(
            binding,
            "runtime external disagrees with its canonical CC signature",
        ));
    }
    Ok(())
}

/// Use the common CC payload protocol at the external call boundary. Source
/// arrays already store erased elements: their identity is never converted.
pub(crate) fn canonical_signature(
    binding: &RuntimeBinding,
    signature: &cc::Signature,
    table: &cc::RepresentationTable,
) -> Result<cc::Signature, Vec<BackendError>> {
    if binding.module != psrs_runtime::STORAGE_MODULE {
        return Err(error(binding, "unknown runtime storage provider"));
    }
    let operation = StorageOperation::from_export(&binding.function)
        .ok_or_else(|| error(binding, "unknown runtime storage export"))?;
    let contract = operation.source_contract();
    if signature.parameters.len() != contract.parameters.len() {
        return Err(error(
            binding,
            "runtime CC signature has the wrong parameter count",
        ));
    }
    let shape = |expected, actual| -> Result<_, Vec<BackendError>> {
        match expected {
            StorageValue::Int | StorageValue::Unit if actual == cc::ValueShape::Integer => {
                Ok(actual)
            }
            StorageValue::Element => Ok(cc::payload::erased_shape()),
            StorageValue::Any => Ok(actual),
            StorageValue::Array => {
                if let cc::ValueShape::Reference(cc::Reference {
                    nullable: false,
                    heap: cc::RefShape::Repr(id),
                }) = actual
                    && matches!(table.representation(id), Some(cc::Representation::Array { element }) if *element == cc::payload::erased_shape())
                {
                    return Ok(actual);
                }
                Err(error(
                    binding,
                    "runtime array requires canonical erased-element storage",
                ))
            }
            _ => Err(error(
                binding,
                "runtime CC operand shape disagrees with its contract",
            )),
        }
    };
    Ok(cc::Signature {
        parameters: contract
            .parameters
            .iter()
            .copied()
            .zip(&signature.parameters)
            .map(|(expected, actual)| shape(expected, *actual))
            .collect::<Result<_, _>>()?,
        result: shape(contract.payload, signature.result)?,
    })
}
