use super::*;

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
fn preserves_switch_selectors_and_all_successors_through_p10() {
    let entry = SymbolId::new(ModuleId(0), 0);
    let function = Function {
        id: FunctionId(0),
        symbol: entry,
        name: "switch_main".into(),
        parameters: vec![ValueId(0)],
        values: vec![
            value(0, ValueType::I32),
            value(1, ValueType::I32),
            value(2, ValueType::I32),
            value(3, ValueType::I32),
            value(4, ValueType::I32),
            value(5, ValueType::I32),
            value(6, ValueType::I32),
        ],
        entry: BlockId(0),
        blocks: vec![
            BasicBlock {
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
                    Instruction::Copy {
                        destination: ValueId(3),
                        value: ValueId(2),
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Switch {
                    value: ValueId(3),
                    cases: vec![(3, BlockId(1))],
                    default: BlockId(2),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: Vec::new(),
                instructions: vec![Instruction::Constant {
                    destination: ValueId(5),
                    value: 23,
                    span: span(),
                }],
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(5)],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: vec![Instruction::Constant {
                    destination: ValueId(6),
                    value: 47,
                    span: span(),
                }],
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(6)],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(3),
                parameters: vec![ValueId(4)],
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: ValueId(4),
                    span: span(),
                }),
            },
        ],
        result: ValueId(4),
        result_type: ValueType::I32,
        span: span(),
    };

    let optimized = optimize(
        module(vec![function], Vec::new(), entry),
        TargetCapabilities::default(),
    )
    .expect("valid MIR Switch should survive P10");
    let function = &optimized.functions[0];
    assert_eq!(
        function.blocks.len(),
        4,
        "both switch arms must remain reachable"
    );
    assert!(
        function.blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(
                instruction,
                Instruction::Primitive {
                    destination: ValueId(2),
                    op: NumericOp::I32Add,
                    ..
                }
            )),
        "the selector computation is live through the Switch"
    );
    assert!(
        matches!(
            function.blocks[0].terminator,
            Some(Terminator::Switch {
                value: ValueId(2),
                ..
            })
        ),
        "copy forwarding must remap the Switch selector"
    );
}
