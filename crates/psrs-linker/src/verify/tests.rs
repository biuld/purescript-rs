//! Contract-verification tests.

use super::*;
use crate::target::{
    ArtifactContract, ArtifactKind, CoreSignature, CoreType, DeclaredExport, DeclaredGlobal,
    DeclaredImport, DeclaredTable, ExportKind, ImportKind, InitializationContract, StorageContract,
    StorageRegion,
};

fn number_format_contract() -> ArtifactContract {
    ArtifactContract {
        id: "psrs:runtime-number-format".into(),
        kind: ArtifactKind::CoreModule,
        module_name: psrs_runtime::MODULE_NAME.into(),
        sha256: psrs_runtime::NUMBER_FORMATTER.provenance.sha256.into(),
        provenance: "test".into(),
        required_features: [
            "mutable-globals",
            "sign-extension",
            "bulk-memory",
            "reference-types",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        imports: vec![DeclaredImport {
            module: psrs_runtime::MEMORY_MODULE.into(),
            field: psrs_runtime::MEMORY_FIELD.into(),
            kind: ImportKind::Memory,
        }],
        exports: vec![
            DeclaredExport {
                name: psrs_runtime::NUMBER_FORMAT.export.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64, CoreType::I32, CoreType::I32],
                    result: Some(CoreType::I32),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::HEAP_BASE_EXPORT.into(),
                kind: ExportKind::Global,
                signature: None,
            },
        ],
        tables: vec![DeclaredTable {
            element: "funcref".into(),
            minimum: 1,
            maximum: Some(1),
        }],
        globals: vec![
            DeclaredGlobal {
                mutable: true,
                initial: psrs_runtime::HEAP_START,
            },
            DeclaredGlobal {
                mutable: false,
                initial: psrs_runtime::HEAP_START,
            },
        ],
        storage: Some(StorageContract {
            static_data: StorageRegion {
                owner: "number-format".into(),
                start: psrs_runtime::RESERVED_START,
                end: psrs_runtime::STACK_BOTTOM,
            },
            stack: StorageRegion {
                owner: "number-format".into(),
                start: psrs_runtime::STACK_BOTTOM,
                end: psrs_runtime::HEAP_START,
            },
            heap_start: psrs_runtime::HEAP_START,
            minimum_pages: 3,
            stack_pointer_global: 0,
            stack_bound_bytes: psrs_runtime::NUMBER_FORMATTER.storage.stack_bound_bytes,
            stack_bound_evidence: psrs_runtime::NUMBER_FORMATTER
                .storage
                .stack_bound_evidence
                .into(),
        }),
        initialization: InitializationContract {
            start_forbidden: true,
            data_range: (psrs_runtime::RESERVED_START, psrs_runtime::STACK_BOTTOM),
        },
        instantiate_after_shims: true,
    }
}

#[test]
fn embedded_artifact_satisfies_its_contract() {
    let contract = number_format_contract();
    verify_artifact(&contract, psrs_runtime::NUMBER_FORMATTER.bytes)
        .expect("the pinned artifact should verify");
}

#[test]
fn a_guest_component_provider_is_rejected_without_a_host_fallback() {
    let mut contract = number_format_contract();
    contract.kind = ArtifactKind::Component;
    let error = verify_artifact(&contract, psrs_runtime::NUMBER_FORMATTER.bytes)
        .expect_err("guest components are not composable yet");
    assert!(
        error.to_string().contains("no silent host fallback"),
        "{error}"
    );
}

#[test]
fn a_stale_digest_is_rejected() {
    let mut contract = number_format_contract();
    contract.sha256 = "0".repeat(64);
    assert!(verify_artifact(&contract, psrs_runtime::NUMBER_FORMATTER.bytes).is_err());
}

#[test]
fn an_undeclared_export_is_rejected() {
    let mut contract = number_format_contract();
    contract.exports.remove(1);
    assert!(verify_artifact(&contract, psrs_runtime::NUMBER_FORMATTER.bytes).is_err());
}

#[test]
fn an_undeclared_import_is_rejected() {
    let mut contract = number_format_contract();
    contract.imports.clear();
    assert!(verify_artifact(&contract, psrs_runtime::NUMBER_FORMATTER.bytes).is_err());
}

#[test]
fn an_overlapping_data_range_is_rejected() {
    let mut contract = number_format_contract();
    contract.initialization.data_range = (0, 16);
    assert!(verify_artifact(&contract, psrs_runtime::NUMBER_FORMATTER.bytes).is_err());
}
