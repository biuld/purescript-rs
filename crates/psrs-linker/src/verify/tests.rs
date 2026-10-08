//! Contract-verification tests.

use super::*;
use crate::target::{
    ArtifactContract, ArtifactKind, CoreSignature, CoreType, DeclaredExport, DeclaredGlobal,
    DeclaredImport, DeclaredTable, ExportKind, ImportKind, InitializationContract, StorageContract,
    StorageRegion,
};

fn number_format_contract() -> ArtifactContract {
    ArtifactContract {
        elements: vec![crate::DeclaredElement {
            table: 0,
            offset: 1,
            functions: vec![54],
        }],
        id: "psrs:runtime-number".into(),
        kind: ArtifactKind::CoreModule,
        module_name: psrs_runtime::MODULE_NAME.into(),
        sha256: psrs_runtime::NUMBER_RUNTIME.provenance.sha256.into(),
        provenance: "test".into(),
        required_features: psrs_runtime::NUMBER_RUNTIME
            .required_features
            .iter()
            .map(|feature| (*feature).to_string())
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
            DeclaredExport {
                name: psrs_runtime::DECIMAL_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::I32, CoreType::I32],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::ACOS_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::ASIN_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::ATAN_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::ATAN2_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64, CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::SIN_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::COS_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::TAN_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::EXP_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::LOG_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::POW_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64, CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::MIN_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64, CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::MAX_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64, CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::SIGN_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::REMAINDER_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64, CoreType::F64],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::IS_NAN_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![CoreType::F64],
                    result: Some(CoreType::I32),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::NAN_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![],
                    result: Some(CoreType::F64),
                }),
            },
            DeclaredExport {
                name: psrs_runtime::INFINITY_EXPORT.into(),
                kind: ExportKind::Func,
                signature: Some(CoreSignature {
                    parameters: vec![],
                    result: Some(CoreType::F64),
                }),
            },
        ],
        tables: vec![DeclaredTable {
            element: "funcref".into(),
            minimum: 2,
            maximum: Some(2),
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
            stack_bound_bytes: psrs_runtime::NUMBER_RUNTIME.storage.stack_bound_bytes,
            stack_bound_evidence: psrs_runtime::NUMBER_RUNTIME
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
    verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes)
        .expect("the pinned artifact should verify");
}

#[test]
fn element_initializers_must_match_the_complete_declared_contract() {
    for change in 0..4 {
        let mut contract = number_format_contract();
        match change {
            0 => contract.elements.clear(),
            1 => contract.elements[0].offset += 1,
            2 => contract.elements[0].table += 1,
            _ => contract.elements[0].functions[0] += 1,
        }
        assert!(
            verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes)
                .unwrap_err()
                .to_string()
                .contains("element segments")
        );
    }
}

#[test]
fn a_component_contract_cannot_use_raw_core_verification() {
    let mut contract = number_format_contract();
    contract.kind = ArtifactKind::Component;
    let error = verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes)
        .expect_err("component bytes cannot satisfy a raw-core contract");
    assert!(
        error.to_string().contains("no silent host fallback"),
        "{error}"
    );
}

#[test]
fn a_stale_digest_is_rejected() {
    let mut contract = number_format_contract();
    contract.sha256 = "0".repeat(64);
    assert!(verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes).is_err());
}

#[test]
fn an_undeclared_export_is_rejected() {
    let mut contract = number_format_contract();
    contract.exports.remove(1);
    assert!(verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes).is_err());
}

#[test]
fn an_undeclared_import_is_rejected() {
    let mut contract = number_format_contract();
    contract.imports.clear();
    assert!(verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes).is_err());
}

#[test]
fn an_overlapping_data_range_is_rejected() {
    let mut contract = number_format_contract();
    contract.initialization.data_range = (0, 16);
    assert!(verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes).is_err());
}

#[test]
fn storage_cannot_name_an_absent_immutable_or_displaced_stack_pointer() {
    for index in [1, 99] {
        let mut contract = number_format_contract();
        contract.storage.as_mut().unwrap().stack_pointer_global = index;
        assert!(verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes).is_err());
    }
    let mut contract = number_format_contract();
    contract.storage.as_mut().unwrap().stack.end += 8;
    assert!(verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes).is_err());
}

#[test]
fn independent_export_table_and_feature_contract_drift_is_rejected() {
    let mut contract = number_format_contract();
    contract.exports[0].signature.as_mut().unwrap().parameters[0] = CoreType::F32;
    assert!(
        verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes)
            .unwrap_err()
            .to_string()
            .contains("signature")
    );
    let mut contract = number_format_contract();
    let mut missing = contract.exports[0].clone();
    missing.name = "absent-export".into();
    contract.exports.push(missing);
    assert!(
        verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes)
            .unwrap_err()
            .to_string()
            .contains("missing declared export")
    );
    let mut contract = number_format_contract();
    contract.tables[0].minimum = 0;
    assert!(
        verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes)
            .unwrap_err()
            .to_string()
            .contains("tables")
    );
    let mut contract = number_format_contract();
    contract.required_features.clear();
    assert!(
        verify_artifact(&contract, psrs_runtime::NUMBER_RUNTIME.bytes)
            .unwrap_err()
            .to_string()
            .contains("not valid Wasm")
    );
}

#[test]
fn a_valid_but_undeclared_eager_initializer_is_rejected() {
    let text = wasmprinter::print_bytes(psrs_runtime::NUMBER_RUNTIME.bytes).unwrap();
    let prefix = text.trim_end().strip_suffix(')').unwrap();
    let bytes = wat::parse_str(format!("{prefix}(func $eager) (start $eager))")).unwrap();
    // The pinned numeric artifact uses saturating truncation. The default
    // validator leaves that proposal off, so this check uses the same feature
    // set as static stack analysis.
    use wasmparser::WasmFeatures as F;
    let features = F::MVP
        | F::MUTABLE_GLOBAL
        | F::SIGN_EXTENSION
        | F::SATURATING_FLOAT_TO_INT
        | F::MULTI_VALUE
        | F::BULK_MEMORY
        | F::REFERENCE_TYPES;
    wasmparser::Validator::new_with_features(features)
        .validate_all(&bytes)
        .expect("valid initializer module");
    let mut contract = number_format_contract();
    contract.sha256 = crate::sha256_hex(&bytes);
    assert!(
        verify_artifact(&contract, &bytes)
            .unwrap_err()
            .to_string()
            .contains("start function")
    );
}
