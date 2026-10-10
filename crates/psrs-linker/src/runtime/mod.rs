//! Converts runtime catalog data into linker-owned artifact contracts.
//!
//! The runtime package owns the raw declaration; the linker owns the typed
//! contract it verifies and plans against.

mod encoded;

use crate::target::{
    ArtifactContract, ArtifactKind, ArtifactReference, CoreSignature, CoreType, DeclaredElement,
    DeclaredExport, DeclaredGlobal, DeclaredImport, DeclaredTable, ExportKind, ImportKind,
    InitializationContract, OfferedOperation, RequiredOperation, RuntimeUnitOffer, StorageContract,
    StorageRegion,
};
use psrs_runtime::{RawType, RuntimeArtifact, RuntimePackage, RuntimeUnit};

/// Maps a catalog artifact into the contract its bytes must satisfy.
pub fn contract(artifact: &RuntimeArtifact) -> ArtifactContract {
    ArtifactContract {
        elements: artifact
            .elements
            .iter()
            .map(|element| DeclaredElement {
                table: element.table,
                offset: element.offset,
                functions: element.functions.to_vec(),
            })
            .collect(),
        id: artifact.id.to_string(),
        kind: ArtifactKind::CoreModule,
        module_name: artifact.module_name.to_string(),
        sha256: artifact.provenance.sha256.to_string(),
        provenance: format!(
            "{} {} via {} ({} {}): {}",
            artifact.provenance.dependency,
            artifact.provenance.dependency_revision,
            artifact.provenance.rust_toolchain,
            artifact.provenance.target,
            artifact.provenance.profile,
            artifact.provenance.recipe,
        ),
        required_features: artifact
            .required_features
            .iter()
            .map(|feature| (*feature).to_string())
            .collect(),
        imports: artifact
            .imports
            .iter()
            .map(|import| DeclaredImport {
                module: import.module.to_string(),
                field: import.field.to_string(),
                kind: match import.kind {
                    psrs_runtime::RawImportKind::Memory { minimum, maximum } => {
                        ImportKind::Memory { minimum, maximum }
                    }
                    psrs_runtime::RawImportKind::Function { parameters, result } => {
                        ImportKind::Function(CoreSignature {
                            parameters: parameters.iter().copied().map(core_type).collect(),
                            result: result.map(core_type),
                        })
                    }
                },
            })
            .collect(),
        exports: artifact
            .function_exports
            .iter()
            .map(|export| DeclaredExport {
                name: export.name.to_string(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: export.parameters.iter().copied().map(core_type).collect(),
                    result: export.result.map(core_type),
                }),
            })
            .chain(artifact.global_exports.iter().map(|name| DeclaredExport {
                name: (*name).to_string(),
                kind: ExportKind::Global,
                signature: None,
            }))
            .collect(),
        tables: artifact
            .tables
            .iter()
            .map(|table| DeclaredTable {
                element: table.element.to_string(),
                minimum: table.minimum,
                maximum: table.maximum,
            })
            .collect(),
        globals: artifact
            .globals
            .iter()
            .map(|(mutable, initial)| DeclaredGlobal {
                mutable: *mutable,
                initial: *initial,
            })
            .collect(),
        storage: Some(StorageContract {
            static_data: StorageRegion {
                owner: artifact.id.to_string(),
                start: artifact.storage.static_data.0,
                end: artifact.storage.static_data.1,
            },
            stack: StorageRegion {
                owner: artifact.id.to_string(),
                start: artifact.storage.stack.0,
                end: artifact.storage.stack.1,
            },
            heap_start: artifact.storage.heap_start,
            minimum_pages: artifact.storage.minimum_pages,
            stack_pointer_global: artifact.storage.stack_pointer_global,
            stack_bound_bytes: artifact.storage.stack_bound_bytes,
            stack_bound_evidence: artifact.storage.stack_bound_evidence.to_string(),
        }),
        initialization: InitializationContract {
            start_forbidden: artifact.start_forbidden,
            data_range: artifact.data_range,
        },
        instantiate_after_shims: artifact.instantiate_after_shims,
    }
}

/// Maps one catalog unit onto the offer planning resolves.
pub fn offer(unit: &RuntimeUnit) -> RuntimeUnitOffer {
    RuntimeUnitOffer {
        id: unit.id.to_string(),
        semantic_contract_version: unit.semantic_contract_version.to_string(),
        provided: unit
            .provided
            .iter()
            .map(|operation| OfferedOperation {
                name: operation.name.to_string(),
                version: operation.version.to_string(),
                export: operation.abi.export.to_string(),
                signature: signature(operation.abi.parameters, operation.abi.result),
            })
            .collect(),
        required: unit
            .required
            .iter()
            .map(|operation| RequiredOperation {
                name: operation.name.to_string(),
                version: operation.version.to_string(),
                signature: signature(operation.parameters, operation.result),
            })
            .collect(),
        artifact: ArtifactReference {
            contract: contract(unit.variant),
            bytes: unit.variant.bytes.to_vec(),
        },
        state_owner: unit.state.owner.map(str::to_string),
        grows_memory: unit.state.grows_memory,
    }
}

/// Maps every unit of a catalog package. Unused units stay unselected.
pub fn package_offers(
    package: &RuntimePackage,
) -> Result<Vec<RuntimeUnitOffer>, crate::LinkErrors> {
    let mut units = package.units.iter().copied().map(offer).collect::<Vec<_>>();
    for unit in package.encoded_units {
        units.push(encoded::offer(unit)?);
    }
    Ok(units)
}

fn signature(parameters: &[RawType], result: Option<RawType>) -> CoreSignature {
    CoreSignature {
        parameters: parameters.iter().copied().map(core_type).collect(),
        result: result.map(core_type),
    }
}

fn core_type(ty: RawType) -> CoreType {
    match ty {
        RawType::I32 => CoreType::I32,
        RawType::I64 => CoreType::I64,
        RawType::F32 => CoreType::F32,
        RawType::F64 => CoreType::F64,
    }
}
