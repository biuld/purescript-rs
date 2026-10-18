//! Capability-inference tests, moved out of `verify/capability.rs` for the
//! 500-line limit. EH-08's `exceptions` inference tests live here too.

use super::super::capability::validate_target_capabilities;
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
        strings: Vec::new(),
        tags: Vec::new(),
        layout: None,
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
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("reference types") && error.span == module.span)
    );
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
        runtime: None,
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

fn module_with_function_type(parameters: Vec<ValueType>, results: Vec<ValueType>) -> mir::Module {
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
        strings: Vec::new(),
        tags: Vec::new(),
        layout: None,
        imports: Vec::new(),
        functions: Vec::new(),
        entry: Some(SymbolId::new(ModuleId(4), 0)),
        span,
    }
}

fn module_with_instruction(instruction: mir::Instruction) -> mir::Module {
    let span = TextRange::new(17, 31);
    mir::Module {
        name: "capability-instruction-test".into(),
        types: Vec::new(),
        strings: Vec::new(),
        tags: Vec::new(),
        layout: None,
        imports: Vec::new(),
        functions: vec![mir::Function {
            id: crate::types::FunctionId(0),
            symbol: SymbolId::new(ModuleId(4), 0),
            name: "capability".into(),
            parameters: Vec::new(),
            values: Vec::new(),
            entry: mir::BlockId(0),
            blocks: vec![mir::BasicBlock {
                id: mir::BlockId(0),
                parameters: Vec::new(),
                instructions: vec![instruction],
                terminator: Some(mir::Terminator::Return {
                    value: crate::types::ValueId(0),
                    span,
                }),
            }],
            result: crate::types::ValueId(0),
            result_type: ValueType::I32,
            frame_weight: 0,
            span,
        }],
        entry: Some(SymbolId::new(ModuleId(4), 0)),
        span,
    }
}

#[test]
fn closure_operations_require_typed_function_references() {
    let module = module_with_instruction(mir::Instruction::ClosureGetCapture {
        destination: crate::types::ValueId(0),
        closure: crate::types::ValueId(1),
        closure_type: crate::types::DefinedTypeId(0),
        capture_array_type: crate::types::DefinedTypeId(1),
        boxed_integer_type: None,
        boxed_f64_type: None,
        index: 0,
        span: TextRange::new(0, 1),
    });
    let target = TargetCapabilities {
        function_references: false,
        ..TargetCapabilities::default()
    };
    let errors = validate_target_capabilities(&module, target)
        .expect_err("a closure operation requires typed function references");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("typed function references")),
        "{errors:?}"
    );
}

#[test]
fn i31_operations_require_reference_types() {
    let module = module_with_instruction(mir::Instruction::I31GetS {
        destination: crate::types::ValueId(0),
        value: crate::types::ValueId(1),
        span: TextRange::new(0, 1),
    });
    let target = TargetCapabilities {
        reference_types: false,
        ..TargetCapabilities::default()
    };
    let errors = validate_target_capabilities(&module, target)
        .expect_err("an i31 operation requires reference types");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("reference types")),
        "{errors:?}"
    );
}

#[test]
fn return_calls_require_the_tail_call_proposal() {
    let span = TextRange::new(17, 31);
    let module = mir::Module {
        name: "capability-tail-test".into(),
        types: Vec::new(),
        strings: Vec::new(),
        tags: Vec::new(),
        layout: None,
        imports: Vec::new(),
        functions: vec![mir::Function {
            id: crate::types::FunctionId(0),
            symbol: SymbolId::new(ModuleId(4), 0),
            name: "tail".into(),
            parameters: Vec::new(),
            values: Vec::new(),
            entry: mir::BlockId(0),
            blocks: vec![mir::BasicBlock {
                id: mir::BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(mir::Terminator::ReturnCall {
                    contract: None,
                    function: SymbolId::new(ModuleId(4), 1),
                    arguments: Vec::new(),
                    span,
                }),
            }],
            result: crate::types::ValueId(0),
            result_type: ValueType::I32,
            frame_weight: 0,
            span,
        }],
        entry: Some(SymbolId::new(ModuleId(4), 0)),
        span,
    };
    let target = TargetCapabilities {
        tail_call: false,
        ..TargetCapabilities::default()
    };
    let errors = validate_target_capabilities(&module, target)
        .expect_err("a tail call requires the tail-call proposal");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("tail-call")),
        "{errors:?}"
    );
}
