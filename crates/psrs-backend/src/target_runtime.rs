//! The embedded target library's implementation catalog entry.
//!
//! This module maps a checked language intrinsic to its embedded artifact
//! export and converts it into a linker-owned requirement. The runtime package
//! owns the raw contract; `psrs-linker` owns verification and planning.

use psrs_hir::{Intrinsic, SymbolId};
use psrs_linker::{
    ArtifactReference, BindingRequirement, Boundary, CoreSignature, CoreType, Provider,
    RequirementId,
};

/// Connects a checked language intrinsic to its embedded implementation.
pub(crate) struct Implementation {
    pub intrinsic: Intrinsic,
    pub symbol: SymbolId,
    pub abi: &'static psrs_runtime::NumericAbi,
    pub artifact: &'static psrs_runtime::RuntimeArtifact,
}

pub(crate) const NUMBER_FORMAT: Implementation = Implementation {
    intrinsic: Intrinsic::NumberToString,
    symbol: crate::abi::NUMBER_TO_STRING_SYMBOL,
    abi: &psrs_runtime::NUMBER_FORMAT,
    artifact: &psrs_runtime::NUMBER_RUNTIME,
};

pub(crate) const NUMBER_PARSE: Implementation = Implementation {
    intrinsic: Intrinsic::NumberFromDecimal,
    symbol: crate::abi::NUMBER_FROM_DECIMAL_SYMBOL,
    abi: &psrs_runtime::NUMBER_PARSE,
    artifact: &psrs_runtime::NUMBER_RUNTIME,
};

pub(crate) const IMPLEMENTATIONS: [&Implementation; 2] = [&NUMBER_FORMAT, &NUMBER_PARSE];

/// The registered implementation for a checked intrinsic, if any.
pub(crate) fn implementation(intrinsic: Intrinsic) -> Option<&'static Implementation> {
    IMPLEMENTATIONS
        .into_iter()
        .find(|implementation| intrinsic == implementation.intrinsic)
}

/// The registered implementation for a MIR import symbol, if any.
pub(crate) fn for_symbol(symbol: SymbolId) -> Option<&'static Implementation> {
    IMPLEMENTATIONS
        .into_iter()
        .find(|implementation| symbol == implementation.symbol)
}

impl Implementation {
    pub fn import(&self) -> crate::mir::Import {
        crate::mir::Import {
            symbol: self.symbol,
            parameters: self
                .abi
                .parameters
                .iter()
                .copied()
                .map(value_type)
                .collect(),
            result: Some(value_type(self.abi.result)),
        }
    }

    pub fn signature(&self) -> CoreSignature {
        CoreSignature {
            parameters: self.abi.parameters.iter().copied().map(core_type).collect(),
            result: Some(core_type(self.abi.result)),
        }
    }

    /// The linker requirement for this artifact export.
    pub fn requirement(&self, id: RequirementId) -> BindingRequirement {
        let signature = self.signature();
        BindingRequirement {
            id,
            origin: format!("{:?}", self.intrinsic),
            boundary: Boundary::RawCore {
                module: self.artifact.module_name.to_string(),
                field: self.abi.export.to_string(),
            },
            expected: Some(signature.clone()),
            provider: Provider::ArtifactExport {
                artifact: self.artifact.id.to_string(),
                export: self.abi.export.to_string(),
                signature,
            },
        }
    }

    /// The pinned artifact and typed contract for this implementation.
    pub fn artifact_reference(&self) -> ArtifactReference {
        ArtifactReference {
            contract: psrs_linker::runtime::contract(self.artifact),
            bytes: self.artifact.bytes.to_vec(),
        }
    }
}

fn core_type(ty: psrs_runtime::RawType) -> CoreType {
    match ty {
        psrs_runtime::RawType::I32 => CoreType::I32,
        psrs_runtime::RawType::I64 => CoreType::I64,
        psrs_runtime::RawType::F32 => CoreType::F32,
        psrs_runtime::RawType::F64 => CoreType::F64,
    }
}

fn value_type(ty: psrs_runtime::RawType) -> crate::types::ValueType {
    match ty {
        psrs_runtime::RawType::I32 => crate::types::ValueType::I32,
        psrs_runtime::RawType::I64 => crate::types::ValueType::I64,
        psrs_runtime::RawType::F32 => crate::types::ValueType::F32,
        psrs_runtime::RawType::F64 => crate::types::ValueType::F64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_number_formatter_maps_to_a_raw_artifact_requirement() {
        let formatter = implementation(Intrinsic::NumberToString).unwrap();
        let requirement = formatter.requirement(RequirementId(0));
        assert_eq!(
            requirement.boundary,
            Boundary::RawCore {
                module: psrs_runtime::MODULE_NAME.into(),
                field: psrs_runtime::NUMBER_EXPORT.into(),
            }
        );
        assert!(matches!(
            requirement.provider,
            Provider::ArtifactExport { .. }
        ));
        assert!(implementation(Intrinsic::I32Add).is_none());
    }
}
