use super::util::mir_error;
use crate::BackendError;
use crate::TargetCapabilities;
use crate::mir;
use crate::types::{CompositeType, HeapType, ValueType};

#[derive(Default)]
struct RequiredCapabilities {
    reference_types: bool,
    function_references: bool,
    gc: bool,
    multi_value: bool,
    tail_call: bool,
    exceptions: bool,
    mutable_globals: bool,
}

/// Checks the capabilities that the current MIR module actually requires.
///
/// The validator says what a binary target accepts; this check rejects an
/// unsupported concrete MIR operation at the boundary that created it.
pub(super) fn validate_target_capabilities(
    module: &mir::Module,
    target: TargetCapabilities,
) -> Result<(), Vec<BackendError>> {
    let mut required = RequiredCapabilities::default();
    for group in &module.types {
        for definition in &group.0 {
            match &definition.composite {
                CompositeType::Func {
                    parameters,
                    results,
                } => {
                    required.multi_value |= results.len() > 1;
                    for ty in parameters.iter().chain(results) {
                        mark_value_type(*ty, &mut required);
                    }
                }
                CompositeType::Struct(fields) => {
                    required.gc = true;
                    for field in fields {
                        mark_storage_type(field.storage, &mut required);
                    }
                }
                CompositeType::Array(field) => {
                    required.gc = true;
                    mark_storage_type(field.storage, &mut required);
                }
            }
        }
    }
    for import in &module.imports {
        for ty in import.parameters.iter().chain(import.result.iter()) {
            mark_value_type(*ty, &mut required);
        }
    }
    for function in &module.functions {
        for value in &function.values {
            mark_value_type(value.ty, &mut required);
        }
        for block in &function.blocks {
            for instruction in &block.instructions {
                mark_instruction(instruction, &mut required);
            }
            if let Some(
                mir::Terminator::ReturnCall { .. } | mir::Terminator::ReturnCallRef { .. },
            ) = &block.terminator
            {
                required.tail_call = true;
            }
            if let Some(
                mir::Terminator::ThrowRef { .. }
                | mir::Terminator::Throw { .. }
                | mir::Terminator::Try { .. },
            ) = &block.terminator
            {
                required.exceptions = true;
            }
        }
    }
    // Declaring a tag commits the module to the exception channel even before
    // any terminator uses it.
    if !module.tags.is_empty() {
        required.exceptions = true;
    }

    let mut errors = Vec::new();
    if required.mutable_globals && !target.mutable_globals {
        errors.extend(mir_error(
            module.span,
            "the selected target does not support mutable globals required by static cells",
        ));
    }
    if required.reference_types && !target.reference_types {
        errors.extend(mir_error(
            module.span,
            "the selected target does not support WebAssembly reference types required by this module",
        ));
    }
    if required.function_references && !target.function_references {
        errors.extend(mir_error(
            module.span,
            "the selected target does not support typed function references required by this module",
        ));
    }
    if required.gc && !target.gc {
        errors.extend(mir_error(
            module.span,
            "the selected target does not support WebAssembly GC required by this module",
        ));
    }
    if required.multi_value && !target.multi_value {
        errors.extend(mir_error(
            module.span,
            "the selected target does not support multi-value function types required by this module",
        ));
    }
    if required.tail_call && !target.tail_call {
        errors.extend(mir_error(
            module.span,
            "the selected target does not support the WebAssembly tail-call proposal required by this module",
        ));
    }
    if required.exceptions && !target.exceptions {
        errors.extend(mir_error(
            module.span,
            "the selected target does not support the WebAssembly exception-handling proposal required by this module",
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(crate::annotate_errors(
            errors,
            module.entry.map(|entry| entry.module),
        ))
    }
}

fn mark_value_type(ty: ValueType, required: &mut RequiredCapabilities) {
    if let ValueType::Ref(reference) = ty {
        mark_reference_type(reference, required);
    }
}

fn mark_storage_type(storage: crate::types::StorageType, required: &mut RequiredCapabilities) {
    if let crate::types::StorageType::Ref(reference) = storage {
        mark_reference_type(reference, required);
    }
}

fn mark_reference_type(reference: crate::types::RefType, required: &mut RequiredCapabilities) {
    required.reference_types = true;
    match reference.heap {
        HeapType::Func => required.function_references = true,
        HeapType::Any
        | HeapType::Eq
        | HeapType::I31
        | HeapType::Struct
        | HeapType::Array
        | HeapType::Index(_) => required.gc = true,
        HeapType::Exn => required.exceptions = true,
        HeapType::Extern => {}
    }
}

fn mark_instruction(instruction: &mir::Instruction, required: &mut RequiredCapabilities) {
    use mir::Instruction;

    match instruction {
        Instruction::StaticCell { .. } => {
            required.mutable_globals = true;
            required.gc = true;
            required.reference_types = true;
        }
        Instruction::RefFunc { .. } | Instruction::CallRef { .. } => {
            required.reference_types = true;
            required.function_references = true;
        }
        Instruction::ClosureNew { .. }
        | Instruction::ClosureCall { .. }
        | Instruction::ClosureGetCapture { .. } => {
            // Closures emit `ref.func`/`call_ref` through a typed function
            // reference, so they require both reference capabilities and GC.
            required.reference_types = true;
            required.function_references = true;
            required.gc = true;
        }
        Instruction::I31New { .. } | Instruction::I31GetS { .. } => {
            required.reference_types = true;
            required.gc = true;
        }
        Instruction::StructNew { .. }
        | Instruction::StructGet { .. }
        | Instruction::StructSet { .. }
        | Instruction::ArrayNew { .. }
        | Instruction::ArrayNewDefault { .. }
        | Instruction::ArrayNewSized { .. }
        | Instruction::ArrayNewFilled { .. }
        | Instruction::ArrayGet { .. }
        | Instruction::ArrayGetU { .. }
        | Instruction::ArrayClone { .. }
        | Instruction::ArraySet { .. }
        | Instruction::ArrayLen { .. }
        // `stringToBytes` builds and reads a GC array.
        | Instruction::StringToBytes { .. } => required.gc = true,
        Instruction::RefNull { .. } | Instruction::RefIsNull { .. } => {
            required.reference_types = true;
        }
        Instruction::RefTest { .. } | Instruction::RefCast { .. } => {
            required.reference_types = true;
            required.gc = true;
        }
        _ => {}
    }
}
