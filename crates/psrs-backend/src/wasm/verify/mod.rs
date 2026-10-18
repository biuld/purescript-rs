//! The thin Wasm IR verifier: module-level invariants live here, and the
//! structured-body walk (label depths, catch clauses, leaf instructions) lives
//! in [`body`].

mod body;

use super::{ExportIndex, ExportKind, GlobalInit, Module, TagIndex, TypeIndex};
use crate::BackendError;
use crate::types::CompositeType;
use psrs_span::TextRange;
use std::collections::HashSet;
use wasm_encoder::{BlockType, RefType, ValType};

pub(super) use body::verify_body;

/// The result type an active label delivers to a branch or catch clause that
/// targets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LabelType {
    /// No result values.
    Empty,
    /// Exactly one result value.
    Value(ValType),
    /// More than one result value, or an unresolvable type index; a catch
    /// clause can never target such a label.
    Many,
}

pub(super) fn structured_label(result: &Option<ValType>) -> LabelType {
    match result {
        Some(ty) => LabelType::Value(*ty),
        None => LabelType::Empty,
    }
}

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
    let mut global_indices = HashSet::new();
    for (position, global) in module.globals.iter().enumerate() {
        if !global_indices.insert(global.index) || global.index.0 != position as u32 {
            errors.push(wasm_error(
                module.span,
                "Wasm global indices are duplicated or not deterministic",
            ));
        }
        let init_matches = match global.init {
            GlobalInit::RefNull(heap) => {
                global.ty
                    == ValType::Ref(RefType {
                        nullable: true,
                        heap_type: heap,
                    })
            }
            GlobalInit::I32(_) => global.ty == ValType::I32,
        };
        if !init_matches {
            errors.push(wasm_error(
                module.span,
                "Wasm global initializer type does not match its value type",
            ));
        }
    }
    for tag in &module.tags {
        match function_signature(module, tag.type_index.0) {
            None => errors.push(wasm_error(tag.span, "Wasm tag type index is out of range")),
            Some((payload, results)) => {
                if !results.is_empty() {
                    errors.push(wasm_error(
                        tag.span,
                        "Wasm tag function type must produce no results",
                    ));
                }
                if payload.len() > 1 {
                    errors.push(wasm_error(
                        tag.span,
                        "Wasm tag payload has more than one value",
                    ));
                }
            }
        }
    }
    let function_count = (module.imports.len()
        + module.functions.len()
        + usize::from(module.entry.is_some())
        + usize::from(module.realloc.is_some())
        + module.helpers.len()) as u32;
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
            (ExportKind::Global, ExportIndex::Global(index)) => {
                (index.0 as usize) < module.globals.len()
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
            &mut vec![function_label_type(module, function.type_index)],
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
            entry.locals.len() as u32,
            function_count,
            module.span,
            &mut vec![function_label_type(module, entry.type_index)],
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
            &mut vec![function_label_type(module, realloc.type_index)],
            &mut errors,
        );
    }
    for helper in &module.helpers {
        if !valid_function_type(module, helper.type_index.0) {
            errors.push(wasm_error(
                helper.span,
                "Wasm helper type index is out of range",
            ));
        }
        let local_count = (helper.parameters.len() + helper.locals.len()) as u32;
        verify_body(
            &helper.body,
            module,
            local_count,
            function_count,
            helper.span,
            &mut vec![function_label_type(module, helper.type_index)],
            &mut errors,
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub(super) fn valid_function_type(module: &Module, index: u32) -> bool {
    function_signature(module, index).is_some()
}

/// The parameter and result value types of the function type behind a type
/// index, resolved across defined types and plain function types. `None` when
/// the index does not name a function type.
pub(super) fn function_signature(
    module: &Module,
    index: u32,
) -> Option<(Vec<ValType>, Vec<ValType>)> {
    let defined = module.defined_type_count();
    if index < defined {
        let definition = module
            .type_defs
            .iter()
            .flat_map(|group| group.0.iter())
            .nth(index as usize)?;
        return match &definition.composite {
            CompositeType::Func {
                parameters,
                results,
            } => Some((
                parameters
                    .iter()
                    .copied()
                    .map(super::convert::val_type)
                    .collect(),
                results
                    .iter()
                    .copied()
                    .map(super::convert::val_type)
                    .collect(),
            )),
            _ => None,
        };
    }
    let ty = module.types.get(index as usize - defined as usize)?;
    Some((ty.parameters.clone(), ty.results.clone()))
}

/// The label type of the implicit function-body label: branching to it returns
/// from the function with its result values.
fn function_label_type(module: &Module, type_index: TypeIndex) -> LabelType {
    match function_signature(module, type_index.0) {
        Some((_, results)) => results_label(&results),
        None => LabelType::Empty,
    }
}

/// The label type of a raw leaf `block`/`loop`/`if`.
pub(super) fn block_type_label(module: &Module, block_type: &BlockType) -> LabelType {
    match block_type {
        BlockType::Empty => LabelType::Empty,
        BlockType::Result(ty) => LabelType::Value(*ty),
        BlockType::FunctionType(index) => match function_signature(module, *index) {
            Some((_, results)) => results_label(&results),
            None => LabelType::Many,
        },
    }
}

fn results_label(results: &[ValType]) -> LabelType {
    match results {
        [] => LabelType::Empty,
        [ty] => LabelType::Value(*ty),
        _ => LabelType::Many,
    }
}

/// The payload a `catch` clause delivers: the tag's function type carries the
/// payload as its parameters. `None` when the tag index is out of range.
pub(super) fn tag_payload(module: &Module, tag: TagIndex) -> Option<LabelType> {
    let definition = module.tags.get(tag.0 as usize)?;
    let (payload, _) = function_signature(module, definition.type_index.0)?;
    Some(results_label(&payload))
}

pub(super) fn wasm_error(span: TextRange, message: &'static str) -> BackendError {
    BackendError::invalid_ir("P11 Wasm verification", span, message)
}
