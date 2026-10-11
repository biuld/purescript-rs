use super::*;

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
fn inlining_uses_the_callee_function_result_value() {
    let caller_symbol = SymbolId::new(ModuleId(0), 0);
    let callee_symbol = SymbolId::new(ModuleId(0), 1);
    let caller = Function {
        id: FunctionId(0),
        symbol: caller_symbol,
        name: "main".into(),
        parameters: Vec::new(),
        values: vec![value(0, ValueType::I32)],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::Call {
                destination: ValueId(0),
                function: callee_symbol,
                arguments: Vec::new(),
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
    let callee = Function {
        id: FunctionId(1),
        symbol: callee_symbol,
        name: "different_result_values".into(),
        parameters: Vec::new(),
        values: vec![value(0, ValueType::I32), value(1, ValueType::I32)],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 10,
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(1),
                    value: 20,
                    span: span(),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(0),
                span: span(),
            }),
        }],
        result: ValueId(1),
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
            .any(|instruction| matches!(instruction, Instruction::Constant { value: 20, .. }))
    );
    assert!(
        !caller.blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::Constant { value: 10, .. }))
    );
}
