use super::*;

fn module_with_branch_guarded_index(object_bytes: u32) -> Module {
    let symbol = SymbolId::new(ModuleId(0), 0);
    let span = span();
    let blocks = vec![
        BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::LinearAlloc {
                    destination: ValueId(1),
                    bytes: 12,
                    alignment: 4,
                    span,
                },
                Instruction::Constant {
                    destination: ValueId(2),
                    value: 0,
                    span,
                },
                Instruction::Primitive {
                    destination: ValueId(3),
                    op: NumericOp::I32LtS,
                    left: ValueId(0),
                    right: ValueId(2),
                    span,
                },
                Instruction::TrapIf {
                    condition: ValueId(3),
                    span,
                },
                Instruction::Constant {
                    destination: ValueId(4),
                    value: 2,
                    span,
                },
                Instruction::Primitive {
                    destination: ValueId(5),
                    op: NumericOp::I32LtS,
                    left: ValueId(0),
                    right: ValueId(4),
                    span,
                },
            ],
            terminator: Some(Terminator::Branch {
                condition: ValueId(5),
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
            ],
            terminator: Some(Terminator::Jump {
                target: BlockId(3),
                arguments: vec![ValueId(11)],
                span,
            }),
        },
        BasicBlock {
            id: BlockId(2),
            parameters: Vec::new(),
            instructions: vec![Instruction::Constant {
                destination: ValueId(12),
                value: 0,
                span,
            }],
            terminator: Some(Terminator::Jump {
                target: BlockId(3),
                arguments: vec![ValueId(12)],
                span,
            }),
        },
        BasicBlock {
            id: BlockId(3),
            parameters: vec![ValueId(13)],
            instructions: Vec::new(),
            terminator: Some(Terminator::Return {
                value: ValueId(13),
                span,
            }),
        },
    ];
    Module {
        name: "BranchGuardedDynamicIndexTest".into(),
        types: Vec::new(),
        imports: Vec::new(),
        functions: vec![crate::mir::Function {
            id: FunctionId(0),
            symbol,
            name: "main".into(),
            parameters: vec![ValueId(0)],
            values: (0..14)
                .map(|index| ValueDecl {
                    id: ValueId(index),
                    ty: if matches!(index, 3 | 5) {
                        ValueType::Boolean
                    } else {
                        ValueType::I32
                    },
                })
                .collect(),
            entry: BlockId(0),
            blocks,
            result: ValueId(13),
            result_type: ValueType::I32,
            span,
        }],
        entry: Some(symbol),
        span,
    }
}

#[test]
fn propagates_true_branch_guards_to_dynamic_array_accesses() {
    let valid = module_with_branch_guarded_index(4);
    crate::mir::verify::verify_module(&valid)
        .expect("the true branch constrains the index to the allocated array");

    let overstated = module_with_branch_guarded_index(8);
    let errors = crate::mir::verify::verify_module(&overstated).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("originating allocation")),
        "{errors:?}"
    );
}
