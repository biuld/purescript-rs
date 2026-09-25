use super::verify_module;
use crate::BackendErrorKind;
use crate::mir::{BasicBlock, BlockId, Function, Instruction, Module, Terminator};
use crate::types::{CompositeType, DefinedType, MemoryId, RecGroup, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn module_with_function(function: Function, types: Vec<RecGroup>) -> Module {
    Module {
        name: "VerifierTest".into(),
        types,
        imports: Vec::new(),
        entry: Some(function.symbol),
        functions: vec![function],
        span: span(),
    }
}

#[test]
fn rejects_constants_with_incompatible_result_types() {
    let function = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "bad_constant".into(),
        parameters: Vec::new(),
        values: vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::F64,
        }],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::Constant {
                destination: ValueId(0),
                value: 1,
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: ValueId(0),
                span: span(),
            }),
        }],
        result: ValueId(0),
        result_type: ValueType::F64,
        span: span(),
    };
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("integer constant")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_unary_primitive_with_mistyped_operands() {
    let input = ValueId(0);
    let output = ValueId(1);
    let function = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "bad_unary".into(),
        parameters: vec![input],
        values: vec![
            ValueDecl {
                id: input,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: output,
                ty: ValueType::I32,
            },
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::UnaryPrimitive {
                destination: output,
                op: crate::mir::UnaryOp::F64Neg,
                value: input,
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: output,
                span: span(),
            }),
        }],
        result: output,
        result_type: ValueType::I32,
        span: span(),
    };
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("unary operand or result type")),
        "{errors:?}"
    );
}

#[test]
fn rejects_duplicate_switch_case_values() {
    let selector = ValueId(0);
    let function = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "duplicate_switch_cases".into(),
        parameters: vec![selector],
        values: vec![ValueDecl {
            id: selector,
            ty: ValueType::I32,
        }],
        entry: BlockId(0),
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Switch {
                    value: selector,
                    cases: vec![(7, BlockId(1)), (7, BlockId(2))],
                    default: BlockId(2),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: selector,
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: selector,
                    span: span(),
                }),
            },
        ],
        result: selector,
        result_type: ValueType::I32,
        span: span(),
    };
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("case values are not unique")),
        "{errors:?}"
    );
}

#[test]
fn rejects_values_used_before_definition() {
    let function = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "bad_order".into(),
        parameters: Vec::new(),
        values: vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(1),
                ty: ValueType::I32,
            },
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Primitive {
                    destination: ValueId(0),
                    op: crate::mir::NumericOp::I32Add,
                    left: ValueId(1),
                    right: ValueId(1),
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(1),
                    value: 1,
                    span: span(),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(0),
                span: span(),
            }),
        }],
        result: ValueId(0),
        result_type: ValueType::I32,
        span: span(),
    };
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("before its definition")),
        "{errors:?}"
    );
}

#[test]
fn rejects_ref_func_with_a_different_target_signature() {
    let function_type = DefinedType {
        final_type: true,
        supertype: None,
        composite: CompositeType::Func {
            parameters: vec![ValueType::I32],
            results: vec![ValueType::I32],
        },
    };
    let callee = SymbolId::new(ModuleId(0), 1);
    let callee_function = Function {
        id: crate::types::FunctionId(1),
        symbol: callee,
        name: "callee".into(),
        parameters: Vec::new(),
        values: vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
        }],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::Constant {
                destination: ValueId(0),
                value: 1,
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: ValueId(0),
                span: span(),
            }),
        }],
        result: ValueId(0),
        result_type: ValueType::I32,
        span: span(),
    };
    let reference = ValueType::Ref(crate::types::RefType {
        nullable: false,
        heap: crate::types::HeapType::Index(crate::types::DefinedTypeId(0)),
    });
    let caller = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "caller".into(),
        parameters: Vec::new(),
        values: vec![ValueDecl {
            id: ValueId(0),
            ty: reference,
        }],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::RefFunc {
                destination: ValueId(0),
                function: callee,
                type_index: crate::types::DefinedTypeId(0),
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: ValueId(0),
                span: span(),
            }),
        }],
        result: ValueId(0),
        result_type: reference,
        span: span(),
    };
    let mut module = module_with_function(caller, vec![RecGroup(vec![function_type])]);
    module.functions.push(callee_function);
    let errors = verify_module(&module).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("does not match its target signature")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_load_from_an_unknown_memory_id() {
    let function = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "bad_memory".into(),
        parameters: Vec::new(),
        values: vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(1),
                ty: ValueType::I32,
            },
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 0,
                    span: span(),
                },
                Instruction::Load {
                    destination: ValueId(1),
                    address: ValueId(0),
                    memory: MemoryId(1),
                    offset: 0,
                    span: span(),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(1),
                span: span(),
            }),
        }],
        result: ValueId(1),
        result_type: ValueType::I32,
        span: span(),
    };
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("unknown memory")),
        "{errors:?}"
    );
}

mod arrays;
mod dictionary;
mod dominance;
mod effects;
mod gaps;
mod scalar;
mod structure;
mod subtype;
mod switch;
mod tail;

#[test]
fn rejects_a_branch_target_with_block_parameters() {
    let condition = ValueId(0);
    let value = ValueId(1);
    let then_parameter = ValueId(2);
    let merge_parameter = ValueId(3);
    let function = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "bad_branch_target".into(),
        parameters: vec![condition, value],
        values: vec![
            ValueDecl {
                id: condition,
                ty: ValueType::Boolean,
            },
            ValueDecl {
                id: value,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: then_parameter,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: merge_parameter,
                ty: ValueType::I32,
            },
        ],
        entry: BlockId(0),
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Branch {
                    condition,
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: vec![then_parameter],
                instructions: Vec::new(),
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![then_parameter],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![value],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(3),
                parameters: vec![merge_parameter],
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: merge_parameter,
                    span: span(),
                }),
            },
        ],
        result: merge_parameter,
        result_type: ValueType::I32,
        span: span(),
    };
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    let error = errors
        .iter()
        .find(|error| {
            error
                .message
                .contains("branch targets cannot have block parameters")
        })
        .expect("a parameterized branch target must be rejected");
    assert_eq!(error.kind, BackendErrorKind::InvalidCompilerIr);
    assert_eq!(error.span, span());
}

#[test]
fn backend_error_kinds_distinguish_invalid_ir_from_unsupported_source() {
    assert_eq!(
        crate::BackendError::invalid_ir("pass", span(), "message").kind,
        BackendErrorKind::InvalidCompilerIr
    );
    assert_eq!(
        crate::BackendError::new("pass", span(), "message").kind,
        BackendErrorKind::UnsupportedSource
    );
}
