//! Host-encoded independent units with separately declared type schemas.
use crate::RawType;

#[derive(Clone, Copy)]
pub enum EncodedHeap {
    Func,
    Extern,
    Any,
    Eq,
    I31,
    Struct,
    Array,
    Defined(u32),
}
#[derive(Clone, Copy)]
pub enum EncodedValue {
    Scalar(RawType),
    Ref { nullable: bool, heap: EncodedHeap },
}
#[derive(Clone)]
pub struct EncodedOperation {
    pub name: String,
    pub version: &'static str,
    pub export: &'static str,
    pub parameters: Vec<EncodedValue>,
    pub result: Option<EncodedValue>,
}

pub struct EncodedRuntimeUnit {
    pub id: &'static str,
    pub version: &'static str,
    pub module_name: &'static str,
    pub provenance: &'static str,
    pub required_features: &'static [&'static str],
    /// A types-only module defining the declaration's reference index space.
    pub type_schema: fn() -> Vec<u8>,
    pub operations: fn() -> Vec<EncodedOperation>,
    pub encode: fn() -> Vec<u8>,
}

#[cfg(feature = "gc-storage")]
pub const STORAGE_UNIT: EncodedRuntimeUnit = EncodedRuntimeUnit {
    id: "psrs:runtime-storage",
    version: "1",
    module_name: crate::STORAGE_MODULE,
    provenance: "deterministic host encoder; storage ABI 1; pinned workspace wasm-encoder",
    required_features: &["gc", "reference-types"],
    type_schema: crate::storage::encode_type_schema,
    operations: storage_operations,
    encode: crate::storage::encode_module,
};

#[cfg(feature = "gc-storage")]
fn storage_operations() -> Vec<EncodedOperation> {
    use crate::{StorageOperation, StorageType};
    let value = |ty| match ty {
        StorageType::I32 => EncodedValue::Scalar(RawType::I32),
        StorageType::Array => EncodedValue::Ref {
            nullable: false,
            heap: EncodedHeap::Defined(0),
        },
        StorageType::Value => EncodedValue::Ref {
            nullable: true,
            heap: EncodedHeap::Eq,
        },
    };
    StorageOperation::ALL
        .into_iter()
        .map(|operation| {
            let abi = operation.abi();
            EncodedOperation {
                name: format!("storage.{}", abi.export),
                version: STORAGE_UNIT.version,
                export: abi.export,
                parameters: abi.parameters.iter().copied().map(value).collect(),
                result: abi.result.map(value),
            }
        })
        .collect()
}
