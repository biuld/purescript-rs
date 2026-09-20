use super::{Body, ExportIndex, ExportKind, Module, Op};
use crate::BackendError;
use crate::types::CompositeType;
use psrs_span::TextRange;
use std::collections::HashSet;
use wasm_encoder::Instruction;

/// Checks the structural invariants of the thin Wasm IR before encoding.
pub fn verify_module(module: &Module) -> Result<(), Vec<BackendError>> {
    let mut errors = Vec::new();
    let mut data_ids = HashSet::new();
    let mut data_indices = HashSet::new();
    for (position, segment) in module.data.iter().enumerate() {
        if !data_ids.insert(segment.id)
            || !data_indices.insert(segment.index)
            || segment.index.0 != position as u32
        {
            errors.push(wasm_error(
                module.span,
                "Wasm data segment IDs or indices are duplicated or not deterministic",
            ));
        }
    }
    let mut memory_ids = HashSet::new();
    let mut memory_indices = HashSet::new();
    for (position, memory) in module.memories.iter().enumerate() {
        if !memory_ids.insert(memory.id)
            || !memory_indices.insert(memory.index)
            || memory.index.0 != position as u32
        {
            errors.push(wasm_error(
                module.span,
                "Wasm memory IDs or indices are not deterministic",
            ));
        }
    }
    let mut table_ids = HashSet::new();
    let mut table_indices = HashSet::new();
    for (position, table) in module.tables.iter().enumerate() {
        if !table_ids.insert(table.id)
            || !table_indices.insert(table.index)
            || table.index.0 != position as u32
        {
            errors.push(wasm_error(
                module.span,
                "Wasm table IDs or indices are not deterministic",
            ));
        }
        if let Some(maximum) = table.maximum
            && maximum < table.minimum
        {
            errors.push(wasm_error(
                module.span,
                "Wasm table maximum is smaller than its minimum",
            ));
        }
    }
    for function in &module.table_elements {
        if function.0 >= function_count_placeholder(module) {
            errors.push(wasm_error(
                module.span,
                "Wasm table element references an unknown function",
            ));
        }
    }
    if !module.table_elements.is_empty() {
        if let Some(table) = module.tables.first() {
            if module.table_elements.len() > table.minimum as usize {
                errors.push(wasm_error(
                    module.span,
                    "Wasm table elements exceed the table minimum",
                ));
            }
        } else {
            errors.push(wasm_error(
                module.span,
                "Wasm table elements require a table resource",
            ));
        }
    }
    let function_count = (module.imports.len()
        + module.functions.len()
        + usize::from(module.entry.is_some())
        + usize::from(module.realloc.is_some())) as u32;
    for import in &module.imports {
        if !valid_function_type(module, import.type_index.0) {
            errors.push(wasm_error(
                module.span,
                "Wasm import type index is out of range",
            ));
        }
    }
    for export in &module.exports {
        let in_range = match (export.kind, export.index) {
            (ExportKind::Function, ExportIndex::Function(index)) => index.0 < function_count,
            (ExportKind::Memory, ExportIndex::Memory(index)) => {
                (index.0 as usize) < module.memories.len()
            }
            _ => false,
        };
        if !in_range {
            errors.push(wasm_error(
                module.span,
                "Wasm export references an unknown index",
            ));
        }
    }
    for function in &module.functions {
        if !valid_function_type(module, function.type_index.0) {
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
        if !valid_function_type(module, entry.type_index.0) {
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
        if !valid_function_type(module, realloc.type_index.0) {
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
        Instruction::CallIndirect {
            type_index,
            table_index,
        } => {
            if !valid_function_type(module, *type_index) {
                errors.push(wasm_error(
                    span,
                    "Wasm call_indirect type index is out of range",
                ));
            }
            if (*table_index as usize) >= module.tables.len() {
                errors.push(wasm_error(
                    span,
                    "Wasm call_indirect table index is out of range",
                ));
            }
        }
        _ => {}
    }
}

fn function_count_placeholder(module: &Module) -> u32 {
    (module.imports.len()
        + module.functions.len()
        + usize::from(module.entry.is_some())
        + usize::from(module.realloc.is_some())) as u32
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
