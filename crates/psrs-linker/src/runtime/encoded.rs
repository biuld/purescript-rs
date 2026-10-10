//! Resolves declared encoded-unit schemas independently of executable bytes.
use crate::target::{
    ArtifactContract, ArtifactKind, ArtifactReference, CoreSignature, DeclaredExport, ExportKind,
    InitializationContract, OfferedOperation, RuntimeUnitOffer,
};
use crate::{CoreTypes, LinkErrors, LinkStage};
use psrs_runtime::{EncodedHeap, EncodedRuntimeUnit, EncodedValue, RawType};
use wasmparser::{AbstractHeapType, HeapType, RefType, ValType};

pub(super) fn offer(unit: &EncodedRuntimeUnit) -> Result<RuntimeUnitOffer, LinkErrors> {
    let resolve = || -> Result<RuntimeUnitOffer, String> {
        let types = CoreTypes::from_module(&(unit.type_schema)())?;
        let provided = (unit.operations)()
            .into_iter()
            .map(|operation| {
                Ok(OfferedOperation {
                    name: operation.name,
                    version: operation.version.into(),
                    export: operation.export.into(),
                    signature: CoreSignature {
                        parameters: operation
                            .parameters
                            .into_iter()
                            .map(|ty| types.value(value(ty)?))
                            .collect::<Result<_, String>>()?,
                        result: operation
                            .result
                            .map(|ty| types.value(value(ty)?))
                            .transpose()?,
                    },
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let bytes = (unit.encode)();
        let contract = ArtifactContract {
            id: unit.id.into(),
            kind: ArtifactKind::CoreModule,
            module_name: unit.module_name.into(),
            sha256: crate::sha256_hex(&bytes),
            provenance: unit.provenance.into(),
            required_features: unit.required_features.iter().map(|s| (*s).into()).collect(),
            imports: vec![],
            exports: provided
                .iter()
                .map(|op| DeclaredExport {
                    name: op.export.clone(),
                    kind: ExportKind::Func,
                    signature: Some(op.signature.clone()),
                })
                .collect(),
            tables: vec![],
            elements: vec![],
            globals: vec![],
            storage: None,
            initialization: InitializationContract {
                start_forbidden: true,
                data_range: (0, 0),
            },
            instantiate_after_shims: false,
        };
        Ok(RuntimeUnitOffer {
            id: unit.id.into(),
            semantic_contract_version: unit.version.into(),
            provided,
            required: vec![],
            artifact: ArtifactReference { contract, bytes },
            state_owner: None,
            grows_memory: false,
        })
    };
    resolve().map_err(|message| LinkErrors::one(LinkStage::Verify, unit.id, message))
}

fn value(value: EncodedValue) -> Result<ValType, String> {
    Ok(match value {
        EncodedValue::Scalar(ty) => match ty {
            RawType::I32 => ValType::I32,
            RawType::I64 => ValType::I64,
            RawType::F32 => ValType::F32,
            RawType::F64 => ValType::F64,
        },
        EncodedValue::Ref { nullable, heap } => {
            let heap = match heap {
                EncodedHeap::Defined(index) => {
                    HeapType::Concrete(wasmparser::UnpackedIndex::Module(index))
                }
                heap => HeapType::Abstract {
                    shared: false,
                    ty: match heap {
                        EncodedHeap::Func => AbstractHeapType::Func,
                        EncodedHeap::Extern => AbstractHeapType::Extern,
                        EncodedHeap::Any => AbstractHeapType::Any,
                        EncodedHeap::Eq => AbstractHeapType::Eq,
                        EncodedHeap::I31 => AbstractHeapType::I31,
                        EncodedHeap::Struct => AbstractHeapType::Struct,
                        EncodedHeap::Array => AbstractHeapType::Array,
                        EncodedHeap::Defined(_) => unreachable!(),
                    },
                },
            };
            ValType::Ref(
                RefType::new(nullable, heap).ok_or("declared reference index is out of range")?,
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_offer_verifies_against_independent_declared_schema() {
        let units = crate::runtime::package_offers(&psrs_runtime::PSRS_RUNTIME).unwrap();
        let storage = units
            .iter()
            .find(|unit| unit.id == psrs_runtime::STORAGE_UNIT.id)
            .unwrap();
        crate::verify_artifact(&storage.artifact.contract, &storage.artifact.bytes).unwrap();
        assert_eq!(
            storage.provided.len(),
            psrs_runtime::StorageOperation::ALL.len()
        );
        assert!(storage.provided.iter().any(|op| {
            op.signature
                .parameters
                .iter()
                .any(|ty| matches!(ty, crate::CoreType::Ref(_)))
        }));
    }

    #[test]
    fn valid_bytes_with_wrong_declared_reference_contract_are_rejected() {
        let mut storage = offer(&psrs_runtime::STORAGE_UNIT).unwrap();
        let export = storage
            .artifact
            .contract
            .exports
            .iter_mut()
            .find(|export| {
                export.signature.as_ref().is_some_and(|sig| {
                    sig.parameters
                        .iter()
                        .any(|ty| matches!(ty, crate::CoreType::Ref(_)))
                })
            })
            .unwrap();
        let signature = export.signature.as_mut().unwrap();
        let parameter = signature
            .parameters
            .iter_mut()
            .find(|ty| matches!(ty, crate::CoreType::Ref(_)))
            .unwrap();
        *parameter = crate::CoreType::I32;
        let error = crate::verify_artifact(&storage.artifact.contract, &storage.artifact.bytes)
            .unwrap_err();
        assert!(error.to_string().contains("signature"), "{error}");
    }
}
