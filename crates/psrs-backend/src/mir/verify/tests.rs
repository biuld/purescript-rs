use super::verify_module;
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
                    op: psrs_core::Primitive::Add,
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
