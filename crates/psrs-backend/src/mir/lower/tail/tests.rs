//! Tail conversion must preserve work and control flow after a call.
use super::*;
use crate::types::FunctionId;
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn block(
    id: u32,
    parameters: Vec<u32>,
    instructions: Vec<Instruction>,
    terminator: Terminator,
) -> BasicBlock {
    BasicBlock {
        id: BlockId(id),
        parameters: parameters.into_iter().map(ValueId).collect(),
        instructions,
        terminator: Some(terminator),
    }
}

fn span() -> TextRange {
    TextRange::new(1, 2)
}

fn call(destination: u32, callee: u32) -> Instruction {
    Instruction::Call {
        contract: None,
        destination: ValueId(destination),
        function: SymbolId::new(ModuleId(0), callee),
        arguments: vec![ValueId(0)],
        span: span(),
    }
}

fn fixture(blocks: Vec<BasicBlock>) -> Function {
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "tail_continuation".into(),
        parameters: vec![ValueId(0)],
        values: (0..4)
            .map(|id| ValueDecl {
                id: ValueId(id),
                ty: ValueType::I32,
            })
            .collect(),
        entry: BlockId(0),
        blocks,
        result: ValueId(1),
        result_type: ValueType::I32,
        frame_weight: 0,
        span: span(),
    }
}

#[test]
fn returned_join_parameter_does_not_discard_a_later_effectful_call() {
    for callee in [0, 1] {
        for tail_call in [false, true] {
            let mut function = fixture(vec![
                block(
                    0,
                    vec![],
                    vec![call(1, callee)],
                    Terminator::Jump {
                        target: BlockId(1),
                        arguments: vec![ValueId(1)],
                        span: span(),
                    },
                ),
                block(
                    1,
                    vec![2],
                    vec![call(3, 2)],
                    Terminator::Return {
                        value: ValueId(2),
                        span: span(),
                    },
                ),
            ]);
            rewrite(
                &mut function,
                TargetCapabilities {
                    tail_call,
                    ..TargetCapabilities::default()
                },
            )
            .unwrap();
            assert!(matches!(
                function.blocks[0].instructions.as_slice(),
                [Instruction::Call { .. }]
            ));
            assert!(matches!(
                function.blocks[0].terminator,
                Some(Terminator::Jump {
                    target: BlockId(1),
                    ..
                })
            ));
        }
    }
}

#[test]
fn a_dominating_return_value_does_not_skip_continuation_work() {
    let mut function = fixture(vec![
        block(
            0,
            vec![],
            vec![call(1, 1)],
            Terminator::Jump {
                target: BlockId(1),
                arguments: vec![],
                span: span(),
            },
        ),
        block(
            1,
            vec![],
            vec![call(3, 2)],
            Terminator::Return {
                value: ValueId(1),
                span: span(),
            },
        ),
    ]);
    rewrite(
        &mut function,
        TargetCapabilities {
            tail_call: true,
            ..TargetCapabilities::default()
        },
    )
    .unwrap();
    assert!(matches!(
        function.blocks[0].instructions.as_slice(),
        [Instruction::Call { .. }]
    ));
}

#[test]
fn a_later_conditional_trap_is_part_of_the_continuation() {
    let mut function = fixture(vec![
        block(
            0,
            vec![],
            vec![call(1, 1)],
            Terminator::Jump {
                target: BlockId(1),
                arguments: vec![],
                span: span(),
            },
        ),
        block(
            1,
            vec![],
            vec![],
            Terminator::Branch {
                condition: ValueId(0),
                then_block: BlockId(2),
                else_block: BlockId(3),
                span: span(),
            },
        ),
        block(
            2,
            vec![],
            vec![],
            Terminator::Return {
                value: ValueId(1),
                span: span(),
            },
        ),
        block(3, vec![], vec![], Terminator::Trap { span: span() }),
    ]);
    rewrite(
        &mut function,
        TargetCapabilities {
            tail_call: true,
            ..TargetCapabilities::default()
        },
    )
    .unwrap();
    assert!(matches!(
        function.blocks[0].instructions.as_slice(),
        [Instruction::Call { .. }]
    ));
}
