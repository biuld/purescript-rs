use super::optimize;
use crate::TargetCapabilities;
use crate::mir::{
    BasicBlock, BlockId, Function, Import, Instruction, Module, NumericOp, Terminator,
};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn value(id: u32, ty: ValueType) -> ValueDecl {
    ValueDecl {
        id: ValueId(id),
        ty,
    }
}

fn module(functions: Vec<Function>, imports: Vec<Import>, entry: SymbolId) -> Module {
    Module {
        name: "OptimizationTest".into(),
        types: Vec::new(),
        imports,
        functions,
        entry: Some(entry),
        span: span(),
    }
}

#[test]
fn folds_total_arithmetic_but_keeps_an_unused_trapping_operation() {
    let symbol = SymbolId::new(ModuleId(0), 0);
    let function = Function {
        id: FunctionId(0),
        symbol,
        name: "main".into(),
        parameters: Vec::new(),
        values: vec![
            value(0, ValueType::I32),
            value(1, ValueType::I32),
            value(2, ValueType::I32),
            value(3, ValueType::I32),
            value(4, ValueType::I32),
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 7,
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(1),
                    value: 2,
                    span: span(),
                },
                Instruction::Primitive {
                    destination: ValueId(2),
                    op: NumericOp::I32Add,
                    left: ValueId(0),
                    right: ValueId(1),
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(3),
                    value: 0,
                    span: span(),
                },
                Instruction::Primitive {
                    destination: ValueId(4),
                    op: NumericOp::I32DivS,
                    left: ValueId(0),
                    right: ValueId(3),
                    span: span(),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(2),
                span: span(),
            }),
        }],
        result: ValueId(2),
        result_type: ValueType::I32,
        span: span(),
    };

    let optimized = optimize(
        module(vec![function], Vec::new(), symbol),
        TargetCapabilities::default(),
    )
    .expect("valid MIR should optimize");
    let instructions = &optimized.functions[0].blocks[0].instructions;
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::Constant {
            destination: ValueId(2),
            value: 9,
            ..
        }
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::Primitive {
            op: NumericOp::I32DivS,
            ..
        }
    )));
}

#[test]
fn folds_branch_merges_prunes_blocks_and_projects_imports() {
    let entry = SymbolId::new(ModuleId(0), 0);
    let import = SymbolId::new(ModuleId(1), 0);
    let function = Function {
        id: FunctionId(0),
        symbol: entry,
        name: "main".into(),
        parameters: Vec::new(),
        values: vec![
            value(0, ValueType::Boolean),
            value(1, ValueType::I32),
            value(2, ValueType::I32),
            value(3, ValueType::I32),
        ],
        entry: BlockId(0),
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![Instruction::Constant {
                    destination: ValueId(0),
                    value: 1,
                    span: span(),
                }],
                terminator: Some(Terminator::Branch {
                    condition: ValueId(0),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                    merge_block: BlockId(3),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: Vec::new(),
                instructions: vec![Instruction::Constant {
                    destination: ValueId(1),
                    value: 11,
                    span: span(),
                }],
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(1)],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::Constant {
                        destination: ValueId(2),
                        value: 99,
                        span: span(),
                    },
                    Instruction::CallVoid {
                        function: import,
                        arguments: Vec::new(),
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(2)],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(3),
                parameters: vec![ValueId(3)],
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: ValueId(3),
                    span: span(),
                }),
            },
        ],
        result: ValueId(3),
        result_type: ValueType::I32,
        span: span(),
    };
    let imports = vec![Import {
        symbol: import,
        parameters: Vec::new(),
        result: None,
    }];

    let optimized = optimize(
        module(vec![function], imports, entry),
        TargetCapabilities::default(),
    )
    .expect("valid MIR should optimize");
    let function = &optimized.functions[0];
    assert_eq!(function.blocks.len(), 3);
    assert!(optimized.imports.is_empty());
    let merge = function
        .blocks
        .iter()
        .find(|block| block.id == BlockId(3))
        .expect("merge block remains reachable");
    assert!(merge.parameters.is_empty());
    assert!(merge.instructions.iter().any(|instruction| matches!(
        instruction,
        Instruction::Constant {
            destination: ValueId(3),
            value: 11,
            ..
        }
    )));
}

#[test]
fn inlines_small_direct_functions_and_folds_the_result() {
    let caller_symbol = SymbolId::new(ModuleId(0), 0);
    let callee_symbol = SymbolId::new(ModuleId(0), 1);
    let caller = Function {
        id: FunctionId(0),
        symbol: caller_symbol,
        name: "main".into(),
        parameters: Vec::new(),
        values: vec![value(0, ValueType::I32), value(1, ValueType::I32)],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 41,
                    span: span(),
                },
                Instruction::Call {
                    destination: ValueId(1),
                    function: callee_symbol,
                    arguments: vec![ValueId(0)],
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
    let callee = Function {
        id: FunctionId(1),
        symbol: callee_symbol,
        name: "increment".into(),
        parameters: vec![ValueId(0)],
        values: vec![
            value(0, ValueType::I32),
            value(1, ValueType::I32),
            value(2, ValueType::I32),
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(1),
                    value: 1,
                    span: span(),
                },
                Instruction::Primitive {
                    destination: ValueId(2),
                    op: NumericOp::I32Add,
                    left: ValueId(0),
                    right: ValueId(1),
                    span: span(),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(2),
                span: span(),
            }),
        }],
        result: ValueId(2),
        result_type: ValueType::I32,
        span: span(),
    };

    let optimized = optimize(
        module(vec![caller, callee], Vec::new(), caller_symbol),
        TargetCapabilities::default(),
    )
    .expect("valid MIR should optimize");
    let caller = &optimized.functions[0];
    assert!(
        caller.blocks[0]
            .instructions
            .iter()
            .all(|instruction| !matches!(instruction, Instruction::Call { .. }))
    );
    assert!(
        caller.blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(
                instruction,
                Instruction::Constant {
                    destination: ValueId(1),
                    value: 42,
                    ..
                }
            ))
    );
}

#[test]
fn copy_forwarding_updates_the_structurer_result_value() {
    let symbol = SymbolId::new(ModuleId(0), 0);
    let function = Function {
        id: FunctionId(0),
        symbol,
        name: "identity".into(),
        parameters: vec![ValueId(0)],
        values: vec![value(0, ValueType::I32), value(1, ValueType::I32)],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::Copy {
                destination: ValueId(1),
                value: ValueId(0),
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: ValueId(1),
                span: span(),
            }),
        }],
        result: ValueId(1),
        result_type: ValueType::I32,
        span: span(),
    };

    let optimized = optimize(
        module(vec![function], Vec::new(), symbol),
        TargetCapabilities::default(),
    )
    .expect("valid MIR should optimize");
    let function = &optimized.functions[0];
    assert_eq!(function.result, ValueId(0));
    assert!(function.blocks[0].instructions.is_empty());
    assert!(matches!(
        function.blocks[0].terminator,
        Some(Terminator::Return {
            value: ValueId(0),
            ..
        })
    ));
}
