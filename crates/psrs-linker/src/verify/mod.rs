//! Artifact contract validation over the actual bytes.
//!
//! The contract declares what a pinned artifact must contain. Verification
//! parses the real module and rejects any stale digest, unexplained export,
//! import, table, global, feature, data range, or initialization path.

use crate::error::{LinkErrors, LinkStage};
use crate::target::{ArtifactContract, ArtifactKind};
use wasmparser::WasmFeatures;

mod parse;
#[cfg(test)]
mod tests;

use parse::{check_contract, parse_core_module};

/// A verified core artifact ready for planning and composition.
#[derive(Clone, Debug)]
pub struct VerifiedArtifact {
    pub id: String,
    pub module_name: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
    pub instantiate_after_shims: bool,
    /// The measured static stack bound, when the artifact declares storage.
    pub stack_bound_bytes: Option<u32>,
}

/// Validates `bytes` against `contract`, returning the verified artifact.
pub fn verify_artifact(
    contract: &ArtifactContract,
    bytes: &[u8],
) -> Result<VerifiedArtifact, LinkErrors> {
    let stage = LinkStage::Verify;
    let id = &contract.id;
    match contract.kind {
        ArtifactKind::CoreModule => {}
        ArtifactKind::Component => {
            return Err(LinkErrors::one(
                stage,
                id,
                "component artifacts require typed component planning, not a raw-core contract; there is no silent host fallback",
            ));
        }
    }
    let digest = crate::sha256_hex(bytes);
    if digest != contract.sha256 {
        return Err(LinkErrors::one(
            stage,
            id,
            format!(
                "artifact digest mismatch: declared {}, actual {digest}",
                contract.sha256
            ),
        ));
    }
    let parsed = parse_core_module(bytes).map_err(|message| LinkErrors::one(stage, id, message))?;
    let features = wasm_features(&contract.required_features)
        .map_err(|message| LinkErrors::one(stage, id, message))?;
    wasmparser::Validator::new_with_features(features)
        .validate_all(bytes)
        .map_err(|error| {
            LinkErrors::one(stage, id, format!("artifact is not valid Wasm: {error}"))
        })?;
    check_contract(contract, &parsed)?;
    let stack_bound_bytes = match &contract.storage {
        Some(storage) => {
            let function_imports: Vec<_> = contract
                .imports
                .iter()
                .filter(|import| matches!(import.kind, crate::ImportKind::Function(_)))
                .collect();
            let stackless_imports = if function_imports.len() == 1
                && crate::plan::is_heap_getter(function_imports[0])
            {
                vec![0]
            } else {
                Vec::new()
            };
            // Application verification discharges this cross-module obligation by
            // requiring the getter to contain only i32.const and end.
            let measured = crate::stack::measure_stack_bound_with_imports(
                bytes,
                storage.stack_pointer_global,
                &stackless_imports,
            )
            .map_err(|message| {
                LinkErrors::one(stage, id, format!("stack analysis failed: {message}"))
            })?;
            if measured.bytes > storage.stack_bound_bytes {
                return Err(LinkErrors::one(
                    stage,
                    id,
                    format!(
                        "artifact stack use {} exceeds its declared {} bytes",
                        measured.bytes, storage.stack_bound_bytes
                    ),
                ));
            }
            Some(measured.bytes)
        }
        None => None,
    };
    Ok(VerifiedArtifact {
        id: contract.id.clone(),
        module_name: contract.module_name.clone(),
        sha256: digest,
        bytes: bytes.to_vec(),
        instantiate_after_shims: contract.instantiate_after_shims,
        stack_bound_bytes,
    })
}

fn wasm_features(names: &[String]) -> Result<WasmFeatures, String> {
    let mut features = WasmFeatures::MVP;
    for name in names {
        features |= match name.as_str() {
            "mutable-globals" => WasmFeatures::MUTABLE_GLOBAL,
            "sign-extension" => WasmFeatures::SIGN_EXTENSION,
            "saturating-float-to-int" => WasmFeatures::SATURATING_FLOAT_TO_INT,
            "multi-value" => WasmFeatures::MULTI_VALUE,
            "bulk-memory" => WasmFeatures::BULK_MEMORY,
            "reference-types" => WasmFeatures::REFERENCE_TYPES,
            "function-references" => WasmFeatures::FUNCTION_REFERENCES,
            "gc" => WasmFeatures::GC,
            other => return Err(format!("unknown required Wasm feature `{other}`")),
        };
    }
    Ok(features)
}
