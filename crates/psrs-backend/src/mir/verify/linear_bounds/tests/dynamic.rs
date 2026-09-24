use super::*;

mod branch_true;

fn module_with_guarded_dynamic_index(object_bytes: u32, upper_guard: bool) -> Module {
    let symbol = SymbolId::new(ModuleId(0), 0);
    let span = span();
    let mut instructions = vec![
        Instruction::LinearAlloc {
            destination: ValueId(1),
            bytes: 12,
            alignment: 4,
            span,
        },
        Instruction::Constant {
            destination: ValueId(12),
            value: 2,
            span,
        },
        Instruction::LinearStore {
            address: ValueId(1),
            value: ValueId(12),
            memory: MemoryId(0),
            offset: 0,
            object_bytes: 12,
            alignment: 4,
            ty: ValueType::I32,
            span,
        },
        Instruction::LinearLoad {
            destination: ValueId(2),
            address: ValueId(1),
            memory: MemoryId(0),
            offset: 0,
            object_bytes: 4,
            alignment: 4,
            ty: ValueType::I32,
            span,
        },
        Instruction::Constant {
            destination: ValueId(3),
            value: 0,
            span,
        },
        Instruction::Primitive {
            destination: ValueId(4),
            op: NumericOp::I32LtS,
            left: ValueId(0),
            right: ValueId(3),
            span,
        },
        Instruction::TrapIf {
            condition: ValueId(4),
            span,
        },
    ];
    if upper_guard {
        instructions.extend([
            Instruction::Primitive {
                destination: ValueId(5),
                op: NumericOp::I32GeS,
                left: ValueId(0),
                right: ValueId(2),
                span,
            },
            Instruction::TrapIf {
                condition: ValueId(5),
                span,
            },
        ]);
    }
    instructions.extend([
        Instruction::Constant {
            destination: ValueId(6),
            value: 4,
            span,
        },
        Instruction::Primitive {
            destination: ValueId(7),
            op: NumericOp::I32Mul,
            left: ValueId(0),
            right: ValueId(6),
            span,
        },
        Instruction::Constant {
            destination: ValueId(8),
            value: 4,
            span,
        },
        Instruction::Primitive {
            destination: ValueId(9),
            op: NumericOp::I32Add,
            left: ValueId(7),
            right: ValueId(8),
            span,
        },
        Instruction::Primitive {
            destination: ValueId(10),
            op: NumericOp::I32Add,
            left: ValueId(1),
            right: ValueId(9),
            span,
        },
        Instruction::LinearLoad {
            destination: ValueId(11),
            address: ValueId(10),
            memory: MemoryId(0),
            offset: 0,
            object_bytes,
            alignment: 4,
            ty: ValueType::I32,
            span,
        },
    ]);
    Module {
        name: "GuardedDynamicLinearIndexTest".into(),
        types: Vec::new(),
        imports: Vec::new(),
        functions: vec![crate::mir::Function {
            id: FunctionId(0),
            symbol,
            name: "main".into(),
            parameters: vec![ValueId(0)],
            values: (0..13)
                .map(|index| ValueDecl {
                    id: ValueId(index),
                    ty: if matches!(index, 4 | 5) {
                        ValueType::Boolean
                    } else {
                        ValueType::I32
                    },
                })
                .collect(),
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions,
                terminator: Some(Terminator::Return {
                    value: ValueId(11),
                    span,
                }),
            }],
            result: ValueId(11),
            result_type: ValueType::I32,
            span,
        }],
        entry: Some(symbol),
        span,
    }
}

fn module_with_dynamic_index_branch_join(
    object_bytes: u32,
    filter_zero_with_equality: bool,
) -> Module {
    let symbol = SymbolId::new(ModuleId(0), 0);
    let span = span();
    let blocks = vec![
        BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::LinearAlloc {
                    destination: ValueId(2),
                    bytes: 12,
                    alignment: 4,
                    span,
                },
                Instruction::Constant {
                    destination: ValueId(3),
                    value: 0,
                    span,
                },
                Instruction::Primitive {
                    destination: ValueId(4),
                    op: NumericOp::I32LtS,
                    left: ValueId(1),
                    right: ValueId(3),
                    span,
                },
                Instruction::TrapIf {
                    condition: ValueId(4),
                    span,
                },
                Instruction::Constant {
                    destination: ValueId(5),
                    value: 2,
                    span,
                },
                Instruction::Primitive {
                    destination: ValueId(6),
                    op: NumericOp::I32GeS,
                    left: ValueId(1),
                    right: ValueId(5),
                    span,
                },
                Instruction::TrapIf {
                    condition: ValueId(6),
                    span,
                },
                Instruction::Constant {
                    destination: ValueId(7),
                    value: 1,
                    span,
                },
            ],
            terminator: Some(Terminator::Branch {
                condition: ValueId(0),
                then_block: BlockId(1),
                else_block: BlockId(2),
                merge_block: BlockId(3),
                span,
            }),
        },
        BasicBlock {
            id: BlockId(1),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Primitive {
                    destination: ValueId(8),
                    op: if filter_zero_with_equality {
                        NumericOp::I32Eq
                    } else {
                        NumericOp::I32Ne
                    },
                    left: ValueId(1),
                    right: ValueId(3),
                    span,
                },
                Instruction::TrapIf {
                    condition: ValueId(8),
                    span,
                },
            ],
            terminator: Some(Terminator::Jump {
                target: BlockId(3),
                arguments: vec![ValueId(1)],
                span,
            }),
        },
        BasicBlock {
            id: BlockId(2),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Primitive {
                    destination: ValueId(9),
                    op: NumericOp::I32Ne,
                    left: ValueId(1),
                    right: ValueId(7),
                    span,
                },
                Instruction::TrapIf {
                    condition: ValueId(9),
                    span,
                },
            ],
            terminator: Some(Terminator::Jump {
                target: BlockId(3),
                arguments: vec![ValueId(1)],
                span,
            }),
        },
        BasicBlock {
            id: BlockId(3),
            parameters: vec![ValueId(10)],
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(11),
                    value: 4,
                    span,
                },
                Instruction::Primitive {
                    destination: ValueId(12),
                    op: NumericOp::I32Mul,
                    left: ValueId(10),
                    right: ValueId(11),
                    span,
                },
                Instruction::Constant {
                    destination: ValueId(13),
                    value: 4,
                    span,
                },
                Instruction::Primitive {
                    destination: ValueId(14),
                    op: NumericOp::I32Add,
                    left: ValueId(12),
                    right: ValueId(13),
                    span,
                },
                Instruction::Primitive {
                    destination: ValueId(15),
                    op: NumericOp::I32Add,
                    left: ValueId(2),
                    right: ValueId(14),
                    span,
                },
                Instruction::LinearLoad {
                    destination: ValueId(16),
                    address: ValueId(15),
                    memory: MemoryId(0),
                    offset: 0,
                    object_bytes,
                    alignment: 4,
                    ty: ValueType::I32,
                    span,
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(16),
                span,
            }),
        },
    ];
    Module {
        name: "DynamicIndexBranchJoinTest".into(),
        types: Vec::new(),
        imports: Vec::new(),
        functions: vec![crate::mir::Function {
            id: FunctionId(0),
            symbol,
            name: "main".into(),
            parameters: vec![ValueId(0), ValueId(1)],
            values: (0..17)
                .map(|index| ValueDecl {
                    id: ValueId(index),
                    ty: if matches!(index, 0 | 4 | 6 | 8 | 9) {
                        ValueType::Boolean
                    } else {
                        ValueType::I32
                    },
                })
                .collect(),
            entry: BlockId(0),
            blocks,
            result: ValueId(16),
            result_type: ValueType::I32,
            span,
        }],
        entry: Some(symbol),
        span,
    }
}

#[test]
fn proves_guarded_dynamic_array_offsets_within_local_allocations() {
    let valid = module_with_guarded_dynamic_index(4, true);
    crate::mir::verify::verify_module(&valid)
        .expect("a guarded index into a two-element local array stays in bounds");

    let overstated = module_with_guarded_dynamic_index(8, true);
    let errors = crate::mir::verify::verify_module(&overstated).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("originating allocation")),
        "{errors:?}"
    );

    let unchecked = module_with_guarded_dynamic_index(4, false);
    let errors = crate::mir::verify::verify_module(&unchecked).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("originating allocation")),
        "{errors:?}"
    );
}

#[test]
fn propagates_guarded_index_ranges_through_branches_and_block_parameters() {
    let bounded_join = module_with_dynamic_index_branch_join(4, false);
    crate::mir::verify::verify_module(&bounded_join)
        .expect("the joined index range should cover both valid array elements");

    let overstated_join = module_with_dynamic_index_branch_join(8, false);
    let errors = crate::mir::verify::verify_module(&overstated_join).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("originating allocation")),
        "{errors:?}"
    );
}

#[test]
fn equality_guards_refine_the_continuing_path_without_losing_the_index() {
    let module = module_with_dynamic_index_branch_join(8, true);
    let errors = crate::mir::verify::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("originating allocation")),
        "{errors:?}"
    );
}
