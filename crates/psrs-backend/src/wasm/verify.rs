use super::{Body, ExportKind, Module, Op};
use crate::BackendError;
use crate::types::CompositeType;
use psrs_span::TextRange;
use wasm_encoder::Instruction;

/// Checks the structural invariants of the thin Wasm IR before encoding.
pub fn verify_module(module: &Module) -> Result<(), Vec<BackendError>> {
    let mut errors = Vec::new();
    let function_count = (module.imports.len()
        + module.functions.len()
        + usize::from(module.entry.is_some())
        + usize::from(module.realloc.is_some())) as u32;
    for import in &module.imports {
        if !valid_function_type(module, import.type_index) {
            errors.push(wasm_error(
                module.span,
                "Wasm import type index is out of range",
            ));
        }
    }
    for export in &module.exports {
        let in_range = match export.kind {
            ExportKind::Function => export.index < function_count,
            ExportKind::Memory => (export.index as usize) < module.memories.len(),
        };
        if !in_range {
            errors.push(wasm_error(
                module.span,
                "Wasm export references an unknown index",
            ));
        }
    }
    for function in &module.functions {
        if !valid_function_type(module, function.type_index) {
            errors.push(wasm_error(
                function.span,
                "Wasm function type index is out of range",
            ));
        }
        let local_count = (function.parameters.len() + function.locals.len()) as u32;
        verify_body(
            &function.body,
            module,
            local_count,
            function_count,
            function.span,
            &mut errors,
        );
    }
    if let Some(entry) = &module.entry {
        if !valid_function_type(module, entry.type_index) {
            errors.push(wasm_error(
                module.span,
                "Wasm entry type index is out of range",
            ));
        }
        verify_body(
            &entry.body,
            module,
            0,
            function_count,
            module.span,
            &mut errors,
        );
    }
    if let Some(realloc) = &module.realloc {
        if !valid_function_type(module, realloc.type_index) {
            errors.push(wasm_error(
                module.span,
                "Wasm realloc type index is out of range",
            ));
        }
        let local_count = (realloc.parameters.len() + realloc.locals.len()) as u32;
        verify_body(
            &realloc.body,
            module,
            local_count,
            function_count,
            realloc.span,
            &mut errors,
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn verify_body(
    body: &Body,
    module: &Module,
    local_count: u32,
    function_count: u32,
    span: TextRange,
    errors: &mut Vec<BackendError>,
) {
    for op in body {
        match op {
            Op::Leaf(instruction) => {
                verify_instruction(
                    instruction,
                    module,
                    local_count,
                    function_count,
                    span,
                    errors,
                );
            }
            Op::If {
                then_body,
                else_body,
                ..
            } => {
                verify_body(then_body, module, local_count, function_count, span, errors);
                verify_body(else_body, module, local_count, function_count, span, errors);
            }
        }
    }
}

fn verify_instruction(
    instruction: &Instruction<'_>,
    module: &Module,
    local_count: u32,
    function_count: u32,
    span: TextRange,
    errors: &mut Vec<BackendError>,
) {
    match instruction {
        Instruction::LocalGet(index)
        | Instruction::LocalSet(index)
        | Instruction::LocalTee(index) => {
            if *index >= local_count {
                errors.push(wasm_error(span, "Wasm local index is out of range"));
            }
        }
        Instruction::Call(index) if *index >= function_count => {
            errors.push(wasm_error(span, "Wasm function index is out of range"));
        }
        Instruction::RefFunc(index) if *index >= function_count => {
            errors.push(wasm_error(span, "Wasm ref.func index is out of range"));
        }
        Instruction::CallRef(index) if !valid_function_type(module, *index) => {
            errors.push(wasm_error(span, "Wasm call_ref type index is out of range"));
        }
        _ => {}
    }
}

fn valid_function_type(module: &Module, index: u32) -> bool {
    let defined = module.defined_type_count();
    if index < defined {
        return module
            .type_defs
            .iter()
            .flat_map(|group| group.0.iter())
            .nth(index as usize)
            .is_some_and(|definition| matches!(definition.composite, CompositeType::Func { .. }));
    }
    index < defined + module.types.len() as u32
}

fn wasm_error(span: TextRange, message: &'static str) -> BackendError {
    BackendError::new("P11 Wasm verification", span, message)
}
