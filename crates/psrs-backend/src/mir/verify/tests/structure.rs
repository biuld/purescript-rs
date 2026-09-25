//! MIR structural negative fixtures: duplicate identifiers, a missing
//! terminator, and a value defined twice are rejected before encoding.

use super::*;

fn simple_function(values: Vec<ValueDecl>, blocks: Vec<BasicBlock>) -> Function {
    Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "structure".into(),
        parameters: Vec::new(),
        values,
        entry: BlockId(0),
        blocks,
        result: ValueId(0),
        result_type: ValueType::I32,
        span: span(),
    }
}

fn return_block(id: u32, value: ValueId) -> BasicBlock {
    BasicBlock {
        id: BlockId(id),
        parameters: Vec::new(),
        instructions: Vec::new(),
        terminator: Some(Terminator::Return {
            value,
            span: span(),
        }),
    }
}

#[test]
fn rejects_duplicate_value_ids() {
    let function = simple_function(
        vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            },
        ],
        vec![return_block(0, ValueId(0))],
    );
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("value IDs are duplicated")),
        "{errors:?}"
    );
}

#[test]
fn rejects_duplicate_block_ids() {
    let function = simple_function(
        vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
        }],
        vec![return_block(0, ValueId(0)), return_block(0, ValueId(0))],
    );
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("block IDs are duplicated")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_block_without_a_terminator() {
    let function = simple_function(
        vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
        }],
        vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: None,
        }],
    );
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("has no terminator")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_value_defined_by_two_instructions() {
    let function = simple_function(
        vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
        }],
        vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 1,
                    span: span(),
                },
                Instruction::Constant {
                    destination: ValueId(0),
                    value: 2,
                    span: span(),
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(0),
                span: span(),
            }),
        }],
    );
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("defined more than once")),
        "{errors:?}"
    );
}
