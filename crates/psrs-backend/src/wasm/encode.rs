use super::{Body, DataMode, ExportIndex, ExportKind, GlobalInit, Module, Op};
use crate::BackendError;
use std::borrow::Cow;
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, DataCountSection, DataSection, ElementSection, Elements,
    EntityType, ExportKind as WasmExportKind, ExportSection, Function as EncoderFunction,
    FunctionSection, GlobalSection, GlobalType, ImportSection, Instruction, MemorySection,
    MemoryType, Module as EncoderModule, TypeSection, ValType,
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
                EntityType::Function(import.type_index.0),
            );
        }
        encoder.section(&imports);
    }

    if has_defined_functions(module) {
        let mut functions = FunctionSection::new();
        for function in &module.functions {
            functions.function(function.type_index.0);
        }
        if let Some(entry) = &module.entry {
            functions.function(entry.type_index.0);
        }
        if let Some(realloc) = &module.realloc {
            functions.function(realloc.type_index.0);
        }
        for helper in &module.helpers {
            functions.function(helper.type_index.0);
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

    if !module.globals.is_empty() {
        let mut globals = GlobalSection::new();
        for global in &module.globals {
            let init = match global.init {
                GlobalInit::RefNull(heap) => ConstExpr::ref_null(heap),
            };
            globals.global(
                GlobalType {
                    val_type: global.ty,
                    mutable: global.mutable,
                    shared: false,
                },
                &init,
            );
        }
        encoder.section(&globals);
    }

    if !module.exports.is_empty() {
        let mut exports = ExportSection::new();
        for export in &module.exports {
            let kind = match export.kind {
                ExportKind::Function => WasmExportKind::Func,
                ExportKind::Memory => WasmExportKind::Memory,
            };
            let index = match export.index {
                ExportIndex::Function(index) => index.0,
                ExportIndex::Memory(index) => index.0,
            };
            exports.export(&export.name, kind, index);
        }
        encoder.section(&exports);
    }

    let function_references = referenced_functions(module);
    if !function_references.is_empty() {
        let mut elements = ElementSection::new();
        elements.declared(Elements::Functions(Cow::Owned(function_references)));
        encoder.section(&elements);
    }

    // `array.new_data` and other bulk-data instructions require the data count
    // section before the code section.
    if !module.data.is_empty() {
        encoder.section(&DataCountSection {
            count: module.data.len() as u32,
        });
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
        for helper in &module.helpers {
            let mut encoded = EncoderFunction::new_with_locals_types(helper.locals.iter().copied());
            emit_body(&helper.body, &mut encoded);
            encoded.instruction(&Instruction::End);
            code.function(&encoded);
        }
        encoder.section(&code);
    }

    if !module.data.is_empty() {
        let mut data = DataSection::new();
        for segment in &module.data {
            match segment.mode {
                DataMode::Active { offset } => {
                    data.active(
                        0,
                        &ConstExpr::i32_const(offset as i32),
                        segment.bytes.iter().copied(),
                    );
                }
                DataMode::Passive => {
                    data.passive(segment.bytes.iter().copied());
                }
            }
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
    for helper in &module.helpers {
        collect_references(&helper.body, &mut references);
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
            Op::Block { body, .. } | Op::Loop { body, .. } => {
                collect_references(body, references);
            }
            Op::Leaf(_) => {}
        }
    }
}

fn has_defined_functions(module: &Module) -> bool {
    !module.functions.is_empty()
        || module.entry.is_some()
        || module.realloc.is_some()
        || !module.helpers.is_empty()
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
            Op::Block { body, result, .. } => {
                let block_type = match result {
                    Some(ty) => BlockType::Result(*ty),
                    None => BlockType::Empty,
                };
                function.instruction(&Instruction::Block(block_type));
                emit_body(body, function);
                function.instruction(&Instruction::End);
            }
            Op::Loop { body, result, .. } => {
                let block_type = match result {
                    Some(ty) => BlockType::Result(*ty),
                    None => BlockType::Empty,
                };
                function.instruction(&Instruction::Loop(block_type));
                emit_body(body, function);
                function.instruction(&Instruction::End);
            }
        }
    }
}
