use super::{Body, Module, Op};
use crate::BackendError;
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function as EncoderFunction,
    FunctionSection, Instruction, Module as EncoderModule, TypeSection,
};

/// Encodes the thin Wasm IR into a binary module.
pub fn encode_module(module: &Module) -> Result<Vec<u8>, Vec<BackendError>> {
    let mut encoder = EncoderModule::new();

    if !module.types.is_empty() {
        let mut types = TypeSection::new();
        for ty in &module.types {
            types
                .ty()
                .function(ty.parameters.iter().copied(), ty.results.iter().copied());
        }
        encoder.section(&types);
    }

    if !module.functions.is_empty() {
        let mut functions = FunctionSection::new();
        for function in &module.functions {
            functions.function(function.type_index);
        }
        encoder.section(&functions);
    }

    if !module.exports.is_empty() {
        let mut exports = ExportSection::new();
        for export in &module.exports {
            exports.export(&export.name, ExportKind::Func, export.function);
        }
        encoder.section(&exports);
    }

    let mut code = CodeSection::new();
    for function in &module.functions {
        let mut encoded = EncoderFunction::new_with_locals_types(function.locals.iter().copied());
        emit_body(&function.body, &mut encoded);
        encoded.instruction(&Instruction::End);
        code.function(&encoded);
    }
    encoder.section(&code);

    Ok(encoder.finish())
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
