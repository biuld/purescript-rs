use super::wasm_error;
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
}

/// Checks the capabilities that the current MIR module actually requires.
///
/// This is intentionally a lowering check rather than a validator setting:
/// the validator says what a target accepts, while this check explains why a
/// particular source program cannot be emitted for a narrower target.
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
    for function in &module.functions {
        for value in &function.values {
            mark_value_type(value.ty, &mut required);
        }
        for block in &function.blocks {
            for instruction in &block.instructions {
                mark_instruction(instruction, &mut required);
            }
        }
    }

    let mut errors = Vec::new();
    if required.reference_types && !target.reference_types {
        errors.extend(wasm_error(
            module.span,
            "the selected target does not support WebAssembly reference types required by this module",
        ));
    }
    if required.function_references && !target.function_references {
        errors.extend(wasm_error(
            module.span,
            "the selected target does not support typed function references required by this module",
        ));
    }
    if required.gc && !target.gc {
        errors.extend(wasm_error(
            module.span,
            "the selected target does not support WebAssembly GC required by this module",
        ));
    }
    if required.multi_value && !target.multi_value {
        errors.extend(wasm_error(
            module.span,
            "the selected target does not support multi-value function types required by this module",
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn mark_value_type(ty: ValueType, required: &mut RequiredCapabilities) {
    if let ValueType::Ref(reference) = ty {
        required.reference_types = true;
        match reference.heap {
            HeapType::Func => required.function_references = true,
            HeapType::I31 | HeapType::Struct | HeapType::Array | HeapType::Index(_) => {
                required.gc = true;
            }
            HeapType::Extern | HeapType::Any | HeapType::Eq => {}
        }
    }
}

fn mark_storage_type(storage: crate::types::StorageType, required: &mut RequiredCapabilities) {
    if let crate::types::StorageType::Ref(reference) = storage {
        required.reference_types = true;
        match reference.heap {
            HeapType::Func => required.function_references = true,
            HeapType::I31 | HeapType::Struct | HeapType::Array | HeapType::Index(_) => {
                required.gc = true;
            }
            HeapType::Extern | HeapType::Any | HeapType::Eq => {}
        }
    }
}

fn mark_instruction(instruction: &mir::Instruction, required: &mut RequiredCapabilities) {
    use mir::Instruction;

    match instruction {
        Instruction::RefFunc { .. } | Instruction::CallRef { .. } => {
            required.reference_types = true;
            required.function_references = true;
        }
        Instruction::ClosureNew { .. }
        | Instruction::ClosureCall { .. }
        | Instruction::ClosureGetCapture { .. }
        | Instruction::I31New { .. }
        | Instruction::I31GetS { .. }
        | Instruction::StructNew { .. }
        | Instruction::StructGet { .. }
        | Instruction::StructSet { .. }
        | Instruction::ArrayNew { .. }
        | Instruction::ArrayGet { .. }
        | Instruction::ArrayClone { .. }
        | Instruction::ArraySet { .. }
        | Instruction::ArrayLen { .. } => required.gc = true,
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

#[cfg(test)]
mod tests {
    use super::validate_target_capabilities;
    use crate::TargetCapabilities;
    use crate::mir;
    use crate::types::{CompositeType, DefinedType, RecGroup};
    use psrs_span::TextRange;

    #[test]
    fn rejects_gc_when_the_target_disables_it() {
        let module = mir::Module {
            name: "gc".into(),
            types: vec![RecGroup(vec![DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Struct(Vec::new()),
            }])],
            imports: Vec::new(),
            functions: Vec::new(),
            entry: None,
            span: TextRange::new(0, 1),
        };
        let target = TargetCapabilities {
            gc: false,
            reference_types: false,
            ..TargetCapabilities::default()
        };
        let errors = validate_target_capabilities(&module, target)
            .expect_err("a GC type must be rejected by a non-GC target");
        assert!(errors.iter().any(|error| error.message.contains("GC")));
    }
}
