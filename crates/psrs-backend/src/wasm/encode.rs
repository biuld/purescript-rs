use super::{Body, ExportKind, Module, Op};
use crate::BackendError;
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, DataSection, EntityType, ExportKind as WasmExportKind,
    ExportSection, Function as EncoderFunction, FunctionSection, ImportSection, Instruction,
    MemorySection, MemoryType, Module as EncoderModule, TypeSection, ValType,
};

/// Encodes the thin Wasm IR into a binary module.
pub fn encode_module(module: &Module) -> Result<Vec<u8>, Vec<BackendError>> {
    let mut encoder = EncoderModule::new();

    if !module.types.is_empty() || !module.type_defs.is_empty() {
        let mut types = TypeSection::new();
        for ty in &module.types {
            types
                .ty()
                .function(ty.parameters.iter().copied(), ty.results.iter().copied());
        }
        for group in &module.type_defs {
            types.ty().rec(group.0.iter().map(super::convert::sub_type));
        }
        encoder.section(&types);
    }

    if !module.imports.is_empty() {
        let mut imports = ImportSection::new();
        for import in &module.imports {
            imports.import(
                &import.module,
                &import.name,
                EntityType::Function(import.type_index),
            );
        }
        encoder.section(&imports);
    }

    if has_defined_functions(module) {
        let mut functions = FunctionSection::new();
        for function in &module.functions {
            functions.function(function.type_index);
        }
        for function in &module.runtime_functions {
            functions.function(function.type_index);
        }
        if let Some(entry) = &module.entry {
            functions.function(entry.type_index);
        }
        encoder.section(&functions);
    }

    if !module.memories.is_empty() {
        let mut memories = MemorySection::new();
        for memory in &module.memories {
            memories.memory(MemoryType {
                minimum: memory.minimum,
                maximum: memory.maximum,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
        }
        encoder.section(&memories);
    }

    if !module.exports.is_empty() {
        let mut exports = ExportSection::new();
        for export in &module.exports {
            let kind = match export.kind {
                ExportKind::Function => WasmExportKind::Func,
                ExportKind::Memory => WasmExportKind::Memory,
            };
            exports.export(&export.name, kind, export.index);
        }
        encoder.section(&exports);
    }

    if has_defined_functions(module) {
        let mut code = CodeSection::new();
        for function in &module.functions {
            let mut encoded =
                EncoderFunction::new_with_locals_types(function.locals.iter().copied());
            emit_body(&function.body, &mut encoded);
            encoded.instruction(&Instruction::End);
            code.function(&encoded);
        }
        for function in &module.runtime_functions {
            let mut encoded =
                EncoderFunction::new_with_locals_types(function.locals.iter().copied());
            emit_body(&function.body, &mut encoded);
            encoded.instruction(&Instruction::End);
            code.function(&encoded);
        }
        if let Some(entry) = &module.entry {
            let mut encoded = EncoderFunction::new_with_locals_types(std::iter::empty::<ValType>());
            emit_body(&entry.body, &mut encoded);
            encoded.instruction(&Instruction::End);
            code.function(&encoded);
        }
        encoder.section(&code);
    }

    if !module.data.is_empty() {
        let mut data = DataSection::new();
        for segment in &module.data {
            data.active(
                0,
                &ConstExpr::i32_const(segment.offset as i32),
                segment.bytes.iter().copied(),
            );
        }
        encoder.section(&data);
    }

    Ok(encoder.finish())
}

fn has_defined_functions(module: &Module) -> bool {
    !module.functions.is_empty() || !module.runtime_functions.is_empty() || module.entry.is_some()
}

fn emit_body(body: &Body, function: &mut EncoderFunction) {
    for op in body {
        match op {
            Op::Leaf(instruction) => {
                function.instruction(instruction);
            }
            Op::If {
                then_body,
                else_body,
                result,
                ..
            } => {
                let block_type = match result {
                    Some(ty) => BlockType::Result(*ty),
                    None => BlockType::Empty,
                };
                function.instruction(&Instruction::If(block_type));
                emit_body(then_body, function);
                function.instruction(&Instruction::Else);
                emit_body(else_body, function);
                function.instruction(&Instruction::End);
            }
        }
    }
}
