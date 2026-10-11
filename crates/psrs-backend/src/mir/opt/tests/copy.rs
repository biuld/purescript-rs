use super::*;

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
