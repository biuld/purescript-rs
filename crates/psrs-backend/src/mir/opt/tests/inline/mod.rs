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

mod budgets;
mod effects;

fn caller_with_calls(
    id: FunctionId,
    symbol: SymbolId,
    callee: SymbolId,
    call_count: usize,
) -> Function {
    caller_with_targets(id, symbol, &vec![callee; call_count])
}

fn caller_with_targets(id: FunctionId, symbol: SymbolId, targets: &[SymbolId]) -> Function {
    let values = (0..targets.len())
        .map(|id| value(id as u32, ValueType::I32))
        .collect::<Vec<_>>();
    let instructions = targets
        .iter()
        .enumerate()
        .map(|(id, callee)| Instruction::Call {
            destination: ValueId(id as u32),
            function: *callee,
            arguments: Vec::new(),
            span: span(),
        })
        .collect::<Vec<_>>();
    let result = ValueId((targets.len() - 1) as u32);
    Function {
        id,
        symbol,
        name: "caller".into(),
        parameters: Vec::new(),
        values,
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions,
            terminator: Some(Terminator::Return {
                value: result,
                span: span(),
            }),
        }],
        result,
        result_type: ValueType::I32,
        span: span(),
    }
}

fn constant_callee(id: FunctionId, symbol: SymbolId, constants: Vec<i32>) -> Function {
    let values = (0..constants.len())
        .map(|id| value(id as u32, ValueType::I32))
        .collect::<Vec<_>>();
    let instructions = constants
        .into_iter()
        .enumerate()
        .map(|(id, value)| Instruction::Constant {
            destination: ValueId(id as u32),
            value,
            span: span(),
        })
        .collect::<Vec<_>>();
    let result = ValueId((values.len() - 1) as u32);
    Function {
        id,
        symbol,
        name: "constant".into(),
        parameters: Vec::new(),
        values,
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions,
            terminator: Some(Terminator::Return {
                value: result,
                span: span(),
            }),
        }],
        result,
        result_type: ValueType::I32,
        span: span(),
    }
}

fn call_count(function: &Function) -> usize {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter(|instruction| matches!(instruction, Instruction::Call { .. }))
        .count()
}

fn span_at(start: u32) -> TextRange {
    TextRange::new(start, start + 1)
}
