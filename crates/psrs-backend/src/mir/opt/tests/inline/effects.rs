use super::*;

#[test]
fn inlining_preserves_trap_and_memory_effect_order_and_spans() {
    let caller_symbol = SymbolId::new(ModuleId(0), 0);
    let callee_symbol = SymbolId::new(ModuleId(0), 1);
    let caller = Function {
        id: FunctionId(0),
        symbol: caller_symbol,
        name: "main".into(),
        parameters: Vec::new(),
        values: vec![
            value(0, ValueType::I32),
            value(1, ValueType::Boolean),
            value(2, ValueType::I32),
            value(3, ValueType::I32),
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 8,
                    span: span_at(1),
                },
                Instruction::Constant {
                    destination: ValueId(1),
                    value: 0,
                    span: span_at(2),
                },
                Instruction::Constant {
                    destination: ValueId(2),
                    value: 33,
                    span: span_at(3),
                },
                Instruction::Store {
                    address: ValueId(0),
                    value: ValueId(2),
                    memory: crate::types::MemoryId(0),
                    offset: 4,
                    span: span_at(20),
                },
                Instruction::Call {
                    destination: ValueId(3),
                    function: callee_symbol,
                    arguments: vec![ValueId(0), ValueId(1), ValueId(2)],
                    span: span_at(30),
                },
                Instruction::Store {
                    address: ValueId(0),
                    value: ValueId(3),
                    memory: crate::types::MemoryId(0),
                    offset: 8,
                    span: span_at(50),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(3),
                span: span_at(60),
            }),
        }],
        result: ValueId(3),
        result_type: ValueType::I32,
        span: span(),
    };
    let callee = Function {
        id: FunctionId(1),
        symbol: callee_symbol,
        name: "write_check_read".into(),
        parameters: vec![ValueId(0), ValueId(1), ValueId(2)],
        values: vec![
            value(0, ValueType::I32),
            value(1, ValueType::Boolean),
            value(2, ValueType::I32),
            value(3, ValueType::I32),
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Store {
                    address: ValueId(0),
                    value: ValueId(2),
                    memory: crate::types::MemoryId(0),
                    offset: 0,
                    span: span_at(40),
                },
                Instruction::TrapIf {
                    condition: ValueId(1),
                    span: span_at(41),
                },
                Instruction::Load {
                    destination: ValueId(3),
                    address: ValueId(0),
                    memory: crate::types::MemoryId(0),
                    offset: 0,
                    span: span_at(42),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(3),
                span: span_at(43),
            }),
        }],
        result: ValueId(3),
        result_type: ValueType::I32,
        span: span(),
    };

    let optimized = optimize(
        module(vec![caller, callee], Vec::new(), caller_symbol),
        TargetCapabilities::default(),
    )
    .expect("inlining effectful MIR must preserve verifier invariants");
    let effects = optimized.functions[0].blocks[0]
        .instructions
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::Store { span, .. }
            | Instruction::Load { span, .. }
            | Instruction::TrapIf { span, .. } => Some(*span),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        effects,
        vec![
            span_at(20),
            span_at(40),
            span_at(41),
            span_at(42),
            span_at(50)
        ]
    );
    assert_eq!(call_count(&optimized.functions[0]), 0);
}
