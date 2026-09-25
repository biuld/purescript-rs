use super::{ACCESS_SPAN, block, decl, function, returning, verify};
use crate::mir::{BasicBlock, BlockId, Instruction, NumericOp, Terminator};
use crate::types::{ValueId, ValueType};

#[test]
fn resolves_wrapping_i32_arithmetic_before_checking_the_region() {
    let function = function(
        vec![
            decl(0, ValueType::I32),
            decl(1, ValueType::I32),
            decl(2, ValueType::I32),
            decl(3, ValueType::I32),
        ],
        Vec::new(),
        vec![block(
            vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: -1,
                    span: ACCESS_SPAN,
                },
                Instruction::Constant {
                    destination: ValueId(1),
                    value: 1,
                    span: ACCESS_SPAN,
                },
                Instruction::Primitive {
                    destination: ValueId(2),
                    op: NumericOp::I32Add,
                    left: ValueId(0),
                    right: ValueId(1),
                    span: ACCESS_SPAN,
                },
                Instruction::Load {
                    destination: ValueId(3),
                    address: ValueId(2),
                    memory: crate::types::MemoryId(0),
                    offset: 0,
                    span: ACCESS_SPAN,
                },
            ],
            returning(ValueId(3)),
        )],
        ValueId(3),
    );

    verify(function).expect("i32 addition wraps to the scratch base address");
}

#[test]
fn propagates_a_known_address_through_block_parameters() {
    let function = function(
        vec![
            decl(0, ValueType::I32),
            decl(1, ValueType::I32),
            decl(2, ValueType::I32),
        ],
        Vec::new(),
        vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![Instruction::StringConstant {
                    destination: ValueId(0),
                    bytes: "abc".into(),
                    span: ACCESS_SPAN,
                }],
                terminator: Some(Terminator::Jump {
                    target: BlockId(1),
                    arguments: vec![ValueId(0)],
                    span: ACCESS_SPAN,
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: vec![ValueId(1)],
                instructions: vec![Instruction::Load {
                    destination: ValueId(2),
                    address: ValueId(1),
                    memory: crate::types::MemoryId(0),
                    offset: 0,
                    span: ACCESS_SPAN,
                }],
                terminator: Some(returning(ValueId(2))),
            },
        ],
        ValueId(2),
    );

    verify(function).expect("the sole incoming edge has the literal's known address");
}

#[test]
fn conflicting_block_parameter_addresses_become_dynamic() {
    let function = function(
        vec![
            decl(0, ValueType::I32),
            decl(1, ValueType::I32),
            decl(2, ValueType::Boolean),
            decl(3, ValueType::I32),
            decl(4, ValueType::I32),
        ],
        vec![ValueId(2)],
        vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::StringConstant {
                        destination: ValueId(0),
                        bytes: "abc".into(),
                        span: ACCESS_SPAN,
                    },
                    Instruction::Constant {
                        destination: ValueId(1),
                        value: 0,
                        span: ACCESS_SPAN,
                    },
                ],
                terminator: Some(Terminator::Branch {
                    condition: ValueId(2),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                    merge_block: BlockId(3),
                    span: ACCESS_SPAN,
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(0)],
                    span: ACCESS_SPAN,
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(1)],
                    span: ACCESS_SPAN,
                }),
            },
            BasicBlock {
                id: BlockId(3),
                parameters: vec![ValueId(3)],
                instructions: vec![Instruction::Load {
                    destination: ValueId(4),
                    address: ValueId(3),
                    memory: crate::types::MemoryId(0),
                    offset: 0,
                    span: ACCESS_SPAN,
                }],
                terminator: Some(returning(ValueId(4))),
            },
        ],
        ValueId(4),
    );

    verify(function).expect("different incoming addresses are treated as dynamic");
}
