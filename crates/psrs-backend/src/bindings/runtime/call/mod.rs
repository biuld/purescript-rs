//! Checked logical CC operands for the runtime-owned raw storage contract.
//!
//! The source storage operations were declared through the withdrawn State
//! protocol. No generic source-to-raw operand projection replaced it, so this
//! boundary reports the missing interface explicitly instead of fabricating a
//! successful binding. Provider identity validation lives in
//! [`super::target::verify_import`], which does not need a State contract.
use crate::BackendError;
use crate::bindings::RuntimeBinding;
use crate::cc;

fn unsupported(binding: &RuntimeBinding, message: &str) -> Vec<BackendError> {
    vec![
        BackendError::invalid_ir("P9 runtime projection", binding.span, message)
            .with_module(binding.source_module),
    ]
}

/// The former State-to-Step operand projection. Retained as an explicit failure
/// while the replacement runtime binding contract is undecided.
pub(crate) fn checked(
    binding: &RuntimeBinding,
    _module: &cc::Module,
) -> Result<(), Vec<BackendError>> {
    Err(unsupported(
        binding,
        "runtime storage bindings require the withdrawn State contract",
    ))
}

/// The former canonical storage signature. There is no generic replacement, so
/// a runtime source binding cannot produce a CC signature.
pub(crate) fn canonical_signature(
    binding: &RuntimeBinding,
    _signature: &cc::Signature,
    _table: &cc::RepresentationTable,
) -> Result<cc::Signature, Vec<BackendError>> {
    Err(unsupported(
        binding,
        "runtime storage bindings require the withdrawn State contract",
    ))
}
