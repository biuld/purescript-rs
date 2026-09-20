//! Conversion from the language-agnostic [`crate::types`] model to the
//! `wasm_encoder` type system. Leaf encoding is delegated to `wasm_encoder`
//! per `docs/decision/DEC-02-thin-structured-wasm-encoding.md`.

use crate::types;
use wasm_encoder::{
    AbstractHeapType, ArrayType, CompositeInnerType, CompositeType, FieldType as WasmFieldType,
    FuncType as WasmFuncType, HeapType, RefType, StorageType as WasmStorageType, StructType,
    SubType, ValType,
};

/// The Wasm value type for a low-level value type.
pub(super) fn val_type(ty: types::ValueType) -> ValType {
    match ty {
        types::ValueType::I32 | types::ValueType::Boolean => ValType::I32,
        types::ValueType::I64 => ValType::I64,
        types::ValueType::F32 => ValType::F32,
        types::ValueType::F64 => ValType::F64,
        types::ValueType::Ref(reference) => ValType::Ref(RefType {
            nullable: reference.nullable,
            heap_type: heap_type(reference.heap),
        }),
    }
}

/// The Wasm heap type for a low-level heap type.
pub(super) fn heap_type(heap: types::HeapType) -> HeapType {
    match heap {
        types::HeapType::Func => abstract_heap(AbstractHeapType::Func),
        types::HeapType::Extern => abstract_heap(AbstractHeapType::Extern),
        types::HeapType::Any => abstract_heap(AbstractHeapType::Any),
        types::HeapType::Eq => abstract_heap(AbstractHeapType::Eq),
        types::HeapType::I31 => abstract_heap(AbstractHeapType::I31),
        types::HeapType::Struct => abstract_heap(AbstractHeapType::Struct),
        types::HeapType::Array => abstract_heap(AbstractHeapType::Array),
        types::HeapType::Index(index) => HeapType::Concrete(index.0),
    }
}

fn abstract_heap(ty: AbstractHeapType) -> HeapType {
    HeapType::Abstract { shared: false, ty }
}

fn storage_type(storage: types::StorageType) -> WasmStorageType {
    match storage {
        types::StorageType::I8 => WasmStorageType::I8,
        types::StorageType::I16 => WasmStorageType::I16,
        types::StorageType::I32 => WasmStorageType::Val(ValType::I32),
        types::StorageType::I64 => WasmStorageType::Val(ValType::I64),
        types::StorageType::F32 => WasmStorageType::Val(ValType::F32),
        types::StorageType::F64 => WasmStorageType::Val(ValType::F64),
        types::StorageType::V128 => WasmStorageType::Val(ValType::V128),
        types::StorageType::Ref(reference) => WasmStorageType::Val(ValType::Ref(RefType {
            nullable: reference.nullable,
            heap_type: heap_type(reference.heap),
        })),
    }
}

fn field_type(field: &types::FieldType) -> WasmFieldType {
    WasmFieldType {
        element_type: storage_type(field.storage),
        mutable: field.mutable,
    }
}

/// The Wasm subtype for a defined type.
pub(super) fn sub_type(def: &types::DefinedType) -> SubType {
    let inner = match &def.composite {
        types::CompositeType::Func {
            parameters,
            results,
        } => CompositeInnerType::Func(WasmFuncType::new(
            parameters.iter().copied().map(val_type),
            results.iter().copied().map(val_type),
        )),
        types::CompositeType::Struct(fields) => CompositeInnerType::Struct(StructType {
            fields: fields.iter().map(field_type).collect(),
        }),
        types::CompositeType::Array(field) => {
            CompositeInnerType::Array(ArrayType(field_type(field)))
        }
    };
    SubType {
        is_final: def.final_type,
        supertype_idx: def.supertype.map(|index| index.0),
        composite_type: CompositeType {
            inner,
            shared: false,
            descriptor: None,
            describes: None,
        },
    }
}
