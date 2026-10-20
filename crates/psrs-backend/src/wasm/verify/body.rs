//! The structured-body walk of the thin Wasm IR verifier: label depths, catch
//! clauses, and leaf-instruction index checks.

use super::super::{Body, CatchClause, Module, Op};
use crate::BackendError;
use psrs_span::TextRange;
use wasm_encoder::Instruction;

use super::{
    LabelType, block_type_label, structured_label, tag_payload, valid_function_type, wasm_error,
};

pub(crate) fn verify_body(
    body: &Body,
    module: &Module,
    local_count: u32,
    function_count: u32,
    span: TextRange,
    labels: &mut Vec<LabelType>,
    errors: &mut Vec<BackendError>,
) {
    let mut raw_label_depth = 0usize;
    for op in body {
        match op {
            Op::Leaf(instruction) => {
                verify_instruction(
                    instruction,
                    module,
                    local_count,
                    function_count,
                    span,
                    labels.len() as u32,
                    errors,
                );
                match instruction {
                    Instruction::Block(block_type)
                    | Instruction::Loop(block_type)
                    | Instruction::If(block_type) => {
                        labels.push(block_type_label(module, block_type));
                        raw_label_depth += 1;
                    }
                    Instruction::End if raw_label_depth > 0 => {
                        labels.pop();
                        raw_label_depth -= 1;
                    }
                    Instruction::End => {
                        errors.push(wasm_error(
                            span,
                            "Wasm end does not close a raw structured label",
                        ));
                    }
                    _ => {}
                }
            }
            Op::If {
                then_body,
                else_body,
                result,
                ..
            } => {
                labels.push(structured_label(result));
                verify_body(
                    then_body,
                    module,
                    local_count,
                    function_count,
                    span,
                    labels,
                    errors,
                );
                verify_body(
                    else_body,
                    module,
                    local_count,
                    function_count,
                    span,
                    labels,
                    errors,
                );
                labels.pop();
            }
            Op::Block { body, result, .. } | Op::Loop { body, result, .. } => {
                labels.push(structured_label(result));
                verify_body(
                    body,
                    module,
                    local_count,
                    function_count,
                    span,
                    labels,
                    errors,
                );
                labels.pop();
            }
            Op::Try { body, clauses, .. } => {
                verify_try_clauses(clauses, module, labels, span, errors);
                // The try_table's own internal label is a branch target inside
                // the protected body; this encoding always gives it an empty
                // block type.
                labels.push(LabelType::Empty);
                verify_body(
                    body,
                    module,
                    local_count,
                    function_count,
                    span,
                    labels,
                    errors,
                );
                labels.pop();
            }
        }
    }
    if raw_label_depth > 0 {
        errors.push(wasm_error(
            span,
            "Wasm raw structured control label is not closed",
        ));
    }
}

/// Checks a `try_table`'s catch clauses against the active-label stack at its
/// emission point. A clause's label is depth-relative to that stack, not
/// counting the `try_table`'s own internal label, so it is resolved here
/// exactly like a `br` depth taken before the region opens.
fn verify_try_clauses(
    clauses: &[CatchClause],
    module: &Module,
    labels: &[LabelType],
    span: TextRange,
    errors: &mut Vec<BackendError>,
) {
    for clause in clauses {
        match clause {
            CatchClause::Catch { tag, label } => {
                if tag.0 as usize >= module.tags.len() {
                    errors.push(wasm_error(
                        span,
                        "Wasm catch clause tag index is out of range",
                    ));
                    continue;
                }
                // A malformed tag definition is already diagnosed in the
                // module-level tag checks; its clause cannot match anything.
                let payload = tag_payload(module, *tag).unwrap_or(LabelType::Many);
                match resolve_label(labels, *label) {
                    None => errors.push(wasm_error(
                        span,
                        "Wasm catch clause label does not target an enclosing label",
                    )),
                    Some(resolved) if resolved == payload => {}
                    Some(_) => errors.push(wasm_error(
                        span,
                        "Wasm catch clause label result does not match the tag payload",
                    )),
                }
            }
            CatchClause::CatchAllRef { label } => {
                let expected =
                    LabelType::Value(wasm_encoder::ValType::Ref(wasm_encoder::RefType {
                        nullable: false,
                        heap_type: wasm_encoder::HeapType::Abstract {
                            shared: false,
                            ty: wasm_encoder::AbstractHeapType::Exn,
                        },
                    }));
                if resolve_label(labels, *label) != Some(expected) {
                    errors.push(wasm_error(
                        span,
                        "Wasm catch_all_ref label must deliver one non-null exception reference",
                    ));
                }
            }
            CatchClause::CatchAll { label } => match resolve_label(labels, *label) {
                None => errors.push(wasm_error(
                    span,
                    "Wasm catch_all clause label does not target an enclosing label",
                )),
                Some(LabelType::Empty) => {}
                Some(_) => errors.push(wasm_error(
                    span,
                    "Wasm catch_all clause label must deliver no values",
                )),
            },
        }
    }
}

/// Resolves a depth-relative label: `0` is the innermost active label.
fn resolve_label(labels: &[LabelType], depth: u32) -> Option<LabelType> {
    let depth = depth as usize;
    if depth >= labels.len() {
        None
    } else {
        Some(labels[labels.len() - 1 - depth])
    }
}

fn verify_instruction(
    instruction: &Instruction<'_>,
    module: &Module,
    local_count: u32,
    function_count: u32,
    span: TextRange,
    label_depth: u32,
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
        Instruction::GlobalGet(index) | Instruction::GlobalSet(index)
            if *index >= module.globals.len() as u32 =>
        {
            errors.push(wasm_error(span, "Wasm global index is out of range"));
        }
        Instruction::RefFunc(index) if *index >= function_count => {
            errors.push(wasm_error(span, "Wasm ref.func index is out of range"));
        }
        Instruction::CallRef(index) if !valid_function_type(module, *index) => {
            errors.push(wasm_error(span, "Wasm call_ref type index is out of range"));
        }
        Instruction::Br(depth) | Instruction::BrIf(depth) if *depth >= label_depth => {
            errors.push(wasm_error(
                span,
                "Wasm branch depth does not target an enclosing label",
            ));
        }
        Instruction::Throw(tag) if *tag >= module.tags.len() as u32 => {
            errors.push(wasm_error(span, "Wasm throw tag index is out of range"));
        }
        Instruction::TryTable(_, catches)
            if catches.iter().any(|catch| match catch {
                wasm_encoder::Catch::One { tag, label }
                | wasm_encoder::Catch::OneRef { tag, label } => {
                    *tag >= module.tags.len() as u32 || *label >= label_depth
                }
                wasm_encoder::Catch::All { label } | wasm_encoder::Catch::AllRef { label } => {
                    *label >= label_depth
                }
            }) =>
        {
            errors.push(wasm_error(
                span,
                "Wasm raw try_table catch clause does not resolve",
            ));
        }
        Instruction::BrTable(targets, default)
            if targets
                .iter()
                .chain(std::iter::once(default))
                .any(|depth| *depth >= label_depth) =>
        {
            errors.push(wasm_error(
                span,
                "Wasm br_table depth does not target an enclosing label",
            ));
        }
        Instruction::CallIndirect { type_index, .. }
            if !valid_function_type(module, *type_index) =>
        {
            errors.push(wasm_error(
                span,
                "Wasm call_indirect type index is out of range",
            ));
        }
        _ => {}
    }
}
