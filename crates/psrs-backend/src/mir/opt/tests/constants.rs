use super::*;

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
