//! The embedded target library's implementation catalog entry.
//!
//! This module maps a checked language intrinsic to its embedded artifact
//! export and converts it into a linker-owned requirement. The runtime package
//! owns the raw contract; `psrs-linker` owns verification and planning.

use psrs_hir::{Intrinsic, SymbolId};
mod language;
use psrs_linker::{
    ArtifactReference, BindingRequirement, Boundary, CoreSignature, CoreType, Provider,
    RequirementId,
};

/// Connects a checked language intrinsic to its embedded implementation.
pub(crate) struct ArtifactImplementation {
    pub intrinsic: Intrinsic,
    pub symbol: SymbolId,
    pub abi: &'static psrs_runtime::RawFunctionAbi,
    pub artifact: &'static psrs_runtime::RuntimeArtifact,
}

pub(crate) const NUMBER_FORMAT: ArtifactImplementation = ArtifactImplementation {
    intrinsic: Intrinsic::NumberToString,
    symbol: crate::abi::NUMBER_TO_STRING_SYMBOL,
    abi: &psrs_runtime::NUMBER_FORMAT,
    artifact: &psrs_runtime::NUMBER_RUNTIME,
};

pub(crate) const NUMBER_PARSE: ArtifactImplementation = ArtifactImplementation {
    intrinsic: Intrinsic::NumberFromDecimal,
    symbol: crate::abi::NUMBER_FROM_DECIMAL_SYMBOL,
    abi: &psrs_runtime::NUMBER_PARSE,
    artifact: &psrs_runtime::NUMBER_RUNTIME,
};

pub(crate) const NUMBER_ACOS: ArtifactImplementation = ArtifactImplementation {
    intrinsic: Intrinsic::NumberAcos,
    symbol: crate::abi::NUMBER_ACOS_SYMBOL,
    abi: &psrs_runtime::NUMBER_ACOS,
    artifact: &psrs_runtime::NUMBER_RUNTIME,
};

pub(crate) const NUMBER_ASIN: ArtifactImplementation = ArtifactImplementation {
    intrinsic: Intrinsic::NumberAsin,
    symbol: crate::abi::NUMBER_ASIN_SYMBOL,
    abi: &psrs_runtime::NUMBER_ASIN,
    artifact: &psrs_runtime::NUMBER_RUNTIME,
};

pub(crate) const NUMBER_ATAN: ArtifactImplementation = ArtifactImplementation {
    intrinsic: Intrinsic::NumberAtan,
    symbol: crate::abi::NUMBER_ATAN_SYMBOL,
    abi: &psrs_runtime::NUMBER_ATAN,
    artifact: &psrs_runtime::NUMBER_RUNTIME,
};

/// The registered implementation for a MIR import symbol, if any.
pub(crate) fn for_symbol(symbol: SymbolId) -> Option<&'static ArtifactImplementation> {
    crate::target_intrinsics::artifacts().find(|implementation| symbol == implementation.symbol)
}

impl ArtifactImplementation {
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
            result: self.abi.result.map(value_type),
        }
    }

    pub fn signature(&self) -> CoreSignature {
        CoreSignature {
            parameters: self.abi.parameters.iter().copied().map(core_type).collect(),
            result: self.abi.result.map(core_type),
        }
    }

    /// The linker requirement for this artifact export.
    pub fn requirement(&self, id: RequirementId, consumer: CoreSignature) -> BindingRequirement {
        let signature = self.signature();
        BindingRequirement {
            id,
            origin: format!("{:?}", self.intrinsic),
            boundary: Boundary::RawCore {
                module: self.artifact.module_name.to_string(),
                field: self.abi.export.to_string(),
            },
            expected: Some(consumer),
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
        let formatter = &NUMBER_FORMAT;
        let requirement = formatter.requirement(RequirementId(0), formatter.signature());
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
        assert!(matches!(
            crate::target_intrinsics::implementation(Intrinsic::IntAdd),
            crate::target_intrinsics::Implementation::Direct(_)
        ));
    }
}

#[cfg(test)]
mod protocol_tests;
