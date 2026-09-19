use super::{Body, ExportKind, Module, Op};
use crate::BackendError;
use std::borrow::Cow;
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, DataSection, ElementSection, Elements, EntityType,
    ExportKind as WasmExportKind, ExportSection, Function as EncoderFunction, FunctionSection,
    ImportSection, Instruction, MemorySection, MemoryType, Module as EncoderModule, TypeSection,
    ValType,
};

/// Encodes the thin Wasm IR into a binary module.
pub fn encode_module(module: &Module) -> Result<Vec<u8>, Vec<BackendError>> {
    let mut encoder = EncoderModule::new();

    if !module.types.is_empty() || !module.type_defs.is_empty() {
        let mut types = TypeSection::new();
        for group in &module.type_defs {
            types.ty().rec(group.0.iter().map(super::convert::sub_type));
        }
        for ty in &module.types {
            types
                .ty()
                .function(ty.parameters.iter().copied(), ty.results.iter().copied());
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
        if let Some(entry) = &module.entry {
            functions.function(entry.type_index);
        }
        if let Some(realloc) = &module.realloc {
            functions.function(realloc.type_index);
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

    let function_references = referenced_functions(module);
    if !function_references.is_empty() {
        let mut elements = ElementSection::new();
        elements.declared(Elements::Functions(Cow::Owned(function_references)));
        encoder.section(&elements);
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
        if let Some(entry) = &module.entry {
            let mut encoded = EncoderFunction::new_with_locals_types(std::iter::empty::<ValType>());
            emit_body(&entry.body, &mut encoded);
            encoded.instruction(&Instruction::End);
            code.function(&encoded);
        }
        if let Some(realloc) = &module.realloc {
            let mut encoded =
                EncoderFunction::new_with_locals_types(realloc.locals.iter().copied());
            emit_body(&realloc.body, &mut encoded);
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

fn referenced_functions(module: &Module) -> Vec<u32> {
    let mut references = Vec::new();
    for function in &module.functions {
        collect_references(&function.body, &mut references);
    }
    if let Some(entry) = &module.entry {
        collect_references(&entry.body, &mut references);
    }
    if let Some(realloc) = &module.realloc {
        collect_references(&realloc.body, &mut references);
    }
    references.sort_unstable();
    references.dedup();
    references
}

fn collect_references(body: &Body, references: &mut Vec<u32>) {
    for op in body {
        match op {
            Op::Leaf(Instruction::RefFunc(function)) => references.push(*function),
            Op::If {
                then_body,
                else_body,
                ..
            } => {
                collect_references(then_body, references);
                collect_references(else_body, references);
            }
            Op::Leaf(_) => {}
        }
    }
}

fn has_defined_functions(module: &Module) -> bool {
    !module.functions.is_empty() || module.entry.is_some() || module.realloc.is_some()
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
