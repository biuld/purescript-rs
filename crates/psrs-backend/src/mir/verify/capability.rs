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
        }
    }

    let mut errors = Vec::new();
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
        HeapType::Extern => {}
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
        | Instruction::ArrayNewDefault { .. }
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
    use crate::types::{CompositeType, DefinedType, HeapType, RecGroup, RefType, ValueType};
    use psrs_hir::{ModuleId, SymbolId};
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

    #[test]
    fn any_and_eq_reference_types_require_gc_with_source_attribution() {
        for heap in [HeapType::Any, HeapType::Eq] {
            let module = module_with_function_type(
                vec![ValueType::Ref(RefType {
                    nullable: true,
                    heap,
                })],
                Vec::new(),
            );
            let target = TargetCapabilities {
                gc: false,
                ..TargetCapabilities::default()
            };

            let errors = validate_target_capabilities(&module, target)
                .expect_err("anyref and eqref require the GC capability");
            assert!(errors.iter().any(|error| {
                error.message.contains("WebAssembly GC")
                    && error.span == module.span
                    && error.module == module.entry.map(|entry| entry.module)
            }));
        }
    }

    #[test]
    fn function_references_require_both_reference_capability_gates() {
        let module = module_with_function_type(
            vec![ValueType::Ref(RefType {
                nullable: true,
                heap: HeapType::Func,
            })],
            Vec::new(),
        );
        let target = TargetCapabilities {
            function_references: false,
            ..TargetCapabilities::default()
        };
        let errors = validate_target_capabilities(&module, target)
            .expect_err("typed function references require function_references");
        assert!(errors.iter().any(|error| {
            error.message.contains("typed function references")
                && error.span == module.span
                && error.module == module.entry.map(|entry| entry.module)
        }));

        let target = TargetCapabilities {
            reference_types: false,
            ..TargetCapabilities::default()
        };
        let errors = validate_target_capabilities(&module, target)
            .expect_err("typed function references also require reference_types");
        assert!(errors.iter().any(|error| {
            error.message.contains("reference types") && error.span == module.span
        }));
    }

    #[test]
    fn extern_references_require_reference_types_but_not_gc() {
        let module = module_with_function_type(
            vec![ValueType::Ref(RefType {
                nullable: true,
                heap: HeapType::Extern,
            })],
            Vec::new(),
        );
        let target = TargetCapabilities {
            gc: false,
            reference_types: false,
            ..TargetCapabilities::default()
        };
        let errors = validate_target_capabilities(&module, target)
            .expect_err("extern references require reference_types");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("reference types"));

        let target = TargetCapabilities {
            gc: false,
            ..TargetCapabilities::default()
        };
        validate_target_capabilities(&module, target).expect("extern references do not require GC");
    }

    #[test]
    fn multi_value_function_types_are_rejected_when_the_gate_is_disabled() {
        let module = module_with_function_type(Vec::new(), vec![ValueType::I32, ValueType::I64]);
        let target = TargetCapabilities {
            multi_value: false,
            ..TargetCapabilities::default()
        };

        let errors = validate_target_capabilities(&module, target)
            .expect_err("multiple function results require multi_value");
        assert!(errors.iter().any(|error| {
            error.message.contains("multi-value")
                && error.span == module.span
                && error.module == module.entry.map(|entry| entry.module)
        }));
    }

    #[test]
    fn imported_function_reference_signatures_are_checked() {
        let mut module = module_with_function_type(Vec::new(), Vec::new());
        module.types.clear();
        module.imports.push(mir::Import {
            symbol: SymbolId::new(ModuleId(4), 9),
            parameters: vec![ValueType::Ref(RefType {
                nullable: true,
                heap: HeapType::Func,
            })],
            result: None,
        });
        let target = TargetCapabilities {
            function_references: false,
            ..TargetCapabilities::default()
        };

        let errors = validate_target_capabilities(&module, target)
            .expect_err("an imported function reference requires its target capability");
        assert!(errors.iter().any(|error| {
            error.message.contains("typed function references")
                && error.span == module.span
                && error.module == module.entry.map(|entry| entry.module)
        }));
    }

    fn module_with_function_type(
        parameters: Vec<ValueType>,
        results: Vec<ValueType>,
    ) -> mir::Module {
        let span = TextRange::new(17, 31);
        mir::Module {
            name: "capability-test".into(),
            types: vec![RecGroup(vec![DefinedType {
                final_type: true,
                supertype: None,
                composite: CompositeType::Func {
                    parameters,
                    results,
                },
            }])],
            imports: Vec::new(),
            functions: Vec::new(),
            entry: Some(SymbolId::new(ModuleId(4), 0)),
            span,
        }
    }
}
