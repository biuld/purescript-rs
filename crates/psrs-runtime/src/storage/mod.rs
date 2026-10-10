//! Executable GC storage provider owned by the runtime package.
//!
//! This host encoder produces an independent core module with its own index
//! space. The application must use the structurally matching mutable eqref
//! array type. It must validate/box payloads before erasure and preserve array
//! identity: copying a typed array to this representation is not a write ABI.
//! Compiler/linker integration requires a checked GC reference call protocol;
//! its encoded unit and separate type schema are advertised by the catalog.

use crate::{StorageOperation, StorageType};
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function, FunctionSection, HeapType,
    Instruction, Module, RefType, StorageType as FieldStorage, TypeSection, ValType,
};

/// Shared structural type contract; no nominal source region is stored here.
pub const ARRAY_TYPE_INDEX: u32 = 0;

fn value_type(ty: StorageType) -> ValType {
    match ty {
        StorageType::I32 => ValType::I32,
        StorageType::Array => ValType::Ref(RefType {
            nullable: false,
            heap_type: HeapType::Concrete(ARRAY_TYPE_INDEX),
        }),
        StorageType::Value => ValType::Ref(RefType::EQREF),
    }
}

/// The data-only declaration schema has an independent module index space.
/// Function contracts resolve their references against this schema, rather
/// than deriving declarations from executable export signatures.
pub fn encode_type_schema() -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .array(&FieldStorage::Val(ValType::Ref(RefType::EQREF)), true);
    module.section(&types);
    module.finish()
}

/// Encode the provider deterministically; no memory, imports, globals or start.
pub fn encode_module() -> Vec<u8> {
    let mut module = Module::new();
    let mut types = TypeSection::new();
    types
        .ty()
        .array(&FieldStorage::Val(ValType::Ref(RefType::EQREF)), true);
    let mut functions = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    for (index, operation) in StorageOperation::ALL.into_iter().enumerate() {
        let abi = operation.abi();
        types.ty().function(
            abi.parameters.iter().copied().map(value_type),
            abi.result.into_iter().map(value_type),
        );
        functions.function(index as u32 + 1);
        exports.export(abi.export, ExportKind::Func, index as u32);
        code.function(&body(operation));
    }
    module.section(&types);
    module.section(&functions);
    module.section(&exports);
    module.section(&code);
    module.finish()
}

fn body(operation: StorageOperation) -> Function {
    let mut f = Function::new([]);
    match operation {
        StorageOperation::Fill => {
            // Source lengths are signed Ints; a negative length must trap
            // before array.new interprets it as an enormous unsigned count.
            f.instruction(&Instruction::LocalGet(0));
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32LtS);
            f.instruction(&Instruction::If(BlockType::Empty));
            f.instruction(&Instruction::Unreachable);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::LocalGet(1));
            f.instruction(&Instruction::LocalGet(0));
            f.instruction(&Instruction::ArrayNew(ARRAY_TYPE_INDEX));
        }
        StorageOperation::Read => {
            f.instruction(&Instruction::LocalGet(0));
            f.instruction(&Instruction::LocalGet(1));
            f.instruction(&Instruction::ArrayGet(ARRAY_TYPE_INDEX));
        }
        StorageOperation::Write => {
            f.instruction(&Instruction::LocalGet(0));
            f.instruction(&Instruction::LocalGet(1));
            f.instruction(&Instruction::LocalGet(2));
            f.instruction(&Instruction::ArraySet(ARRAY_TYPE_INDEX));
        }
        StorageOperation::Trap => {
            f.instruction(&Instruction::Unreachable);
        }
    }
    f.instruction(&Instruction::End);
    f
}

#[cfg(test)]
mod tests;
