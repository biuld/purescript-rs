//! Ordinary CC call signatures for the runtime storage ABI.
use crate::{BackendError, RuntimeBinding, cc};
use psrs_runtime::{RecordSourceValue, StorageOperation, StorageValue};

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
    if binding.module == psrs_runtime::RECORD_STORAGE_MODULE {
        return canonical_record_signature(binding, signature, table);
    }
    if binding.module == super::exception::RUNTIME_EXCEPTION_MODULE {
        return super::exception::canonical_signature(binding, signature, table);
    }
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

fn canonical_record_signature(
    binding: &RuntimeBinding,
    signature: &cc::Signature,
    table: &cc::RepresentationTable,
) -> Result<cc::Signature, Vec<BackendError>> {
    let operation = psrs_runtime::RecordPrimitive::from_export(&binding.function)
        .ok_or_else(|| error(binding, "unknown record-storage export"))?;
    let storage = table.record_storage().ok_or_else(|| {
        error(
            binding,
            "record-storage provider has no shared owner schema",
        )
    })?;
    let contract = operation.source_contract();
    if signature.parameters.len() != contract.parameters.len() {
        return Err(error(
            binding,
            "record primitive CC signature has wrong arity",
        ));
    }
    let record_shape = cc::ValueShape::Reference(cc::Reference {
        nullable: false,
        heap: cc::RefShape::Repr(storage.representation()),
    });
    let shape = |expected, actual| -> Result<_, Vec<BackendError>> {
        match expected {
            RecordSourceValue::Record => {
                let cc::ValueShape::Reference(cc::Reference {
                    nullable: false,
                    heap: cc::RefShape::Repr(representation),
                }) = actual
                else {
                    return Err(error(
                        binding,
                        "record operand has no canonical record shape",
                    ));
                };
                if representation != storage.representation()
                    && !matches!(
                        table.representation(representation),
                        Some(cc::Representation::Product { .. })
                    )
                {
                    return Err(error(
                        binding,
                        "record operand is not a closed record product",
                    ));
                }
                Ok(record_shape)
            }
            RecordSourceValue::Int if actual == cc::ValueShape::Integer => Ok(actual),
            RecordSourceValue::String if actual == cc::ValueShape::String => Ok(actual),
            RecordSourceValue::Slot if actual == cc::payload::erased_shape() => Ok(actual),
            RecordSourceValue::ArrayString | RecordSourceValue::ArraySlot => {
                if let cc::ValueShape::Reference(cc::Reference {
                    nullable: false,
                    heap: cc::RefShape::Repr(id),
                }) = actual
                    && matches!(table.representation(id), Some(cc::Representation::Array { element }) if *element == cc::payload::erased_shape())
                {
                    Ok(actual)
                } else {
                    Err(error(
                        binding,
                        "record primitive arrays require erased-element storage",
                    ))
                }
            }
            _ => Err(error(
                binding,
                "record primitive CC shape disagrees with source policy",
            )),
        }
    };
    let parameters = contract
        .parameters
        .iter()
        .copied()
        .zip(signature.parameters.iter().copied())
        .map(|(expected, actual)| shape(expected, actual))
        .collect::<Result<Vec<_>, _>>()?;
    let result = shape(contract.result, signature.result)?;
    let canonical = cc::Signature { parameters, result };
    if canonical.parameters.len() != signature.parameters.len() {
        return Err(error(
            binding,
            "record primitive CC signature has the wrong parameter count",
        ));
    }
    Ok(canonical)
}
