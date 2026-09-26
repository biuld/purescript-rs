//! Positive dominance fixtures: a valid diamond join and a loop-carried block
//! parameter verify, while a value defined in one branch and used at the join
//! is rejected.

use super::verify_module;
use crate::mir::{BasicBlock, BlockId, Function, Instruction, Module, NumericOp, Terminator};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn module_with(function: Function) -> Module {
    Module {
        name: "DominanceTest".into(),
        types: Vec::new(),
        strings: Vec::new(),
        imports: Vec::new(),
        entry: Some(function.symbol),
        functions: vec![function],
        span: span(),
    }
}

fn function(values: Vec<ValueDecl>, blocks: Vec<BasicBlock>) -> Function {
    Function {
        id: FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "dominance".into(),
        parameters: Vec::new(),
        values,
        entry: BlockId(0),
        blocks,
        result: ValueId(99),
        result_type: ValueType::I32,
        span: span(),
    }
}

#[test]
fn accepts_a_diamond_join_that_receives_a_dominating_value() {
    // B0 defines v then branches; both arms pass v to the join.
    let function = function(
        vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(1),
                ty: ValueType::Boolean,
            },
            ValueDecl {
                id: ValueId(2),
                ty: ValueType::I32,
            },
        ],
        vec![
            BasicBlock {
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
                        value: 1,
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Branch {
                    condition: ValueId(1),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(0)],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: vec![ValueId(0)],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(3),
                parameters: vec![ValueId(2)],
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: ValueId(2),
                    span: span(),
                }),
            },
        ],
    );
    verify_module(&module_with(function)).expect("a dominating join value must verify");
}

#[test]
fn accepts_a_loop_with_a_carried_block_parameter() {
    // B1 carries p; B2 copies p and loops back to B1.
    let function = function(
        vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(1),
                ty: ValueType::Boolean,
            },
            ValueDecl {
                id: ValueId(2),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(3),
                ty: ValueType::I32,
            },
        ],
        vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![Instruction::Constant {
                    destination: ValueId(0),
                    value: 0,
                    span: span(),
                }],
                terminator: Some(Terminator::Jump {
                    target: BlockId(1),
                    arguments: vec![ValueId(0)],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: vec![ValueId(2)],
                instructions: vec![Instruction::Primitive {
                    destination: ValueId(1),
                    op: NumericOp::I32Eq,
                    left: ValueId(2),
                    right: ValueId(0),
                    span: span(),
                }],
                terminator: Some(Terminator::Branch {
                    condition: ValueId(1),
                    then_block: BlockId(3),
                    else_block: BlockId(2),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: vec![Instruction::Copy {
                    destination: ValueId(3),
                    value: ValueId(2),
                    span: span(),
                }],
                terminator: Some(Terminator::Jump {
                    target: BlockId(1),
                    arguments: vec![ValueId(3)],
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(3),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: ValueId(2),
                    span: span(),
                }),
            },
        ],
    );
    verify_module(&module_with(function)).expect("a loop-carried parameter must verify");
}

#[test]
fn rejects_a_value_defined_in_one_branch_and_used_at_the_join() {
    // `projected` is defined only in B1 but read by the return in B3, which B1
    // does not dominate.
    let function = function(
        vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::Boolean,
            },
            ValueDecl {
                id: ValueId(1),
                ty: ValueType::I32,
            },
        ],
        vec![
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
                    value: 7,
                    span: span(),
                }],
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: Vec::new(),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Jump {
                    target: BlockId(3),
                    arguments: Vec::new(),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(3),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: ValueId(1),
                    span: span(),
                }),
            },
        ],
    );
    let errors = verify_module(&module_with(function)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("dominance")),
        "{errors:?}"
    );
}
