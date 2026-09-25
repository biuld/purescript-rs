use super::{module_with_function, span, verify_module};
use crate::mir::{BasicBlock, BlockId, Function, Instruction, NumericOp, Terminator};
use crate::types::{ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};

#[test]
fn rejects_a_switch_with_a_non_i32_selector() {
    let selector = ValueId(0);
    let function = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "non_i32_switch_selector".into(),
        parameters: vec![selector],
        values: vec![ValueDecl {
            id: selector,
            ty: ValueType::Boolean,
        }],
        entry: BlockId(0),
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Switch {
                    value: selector,
                    cases: vec![(0, BlockId(1))],
                    default: BlockId(1),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: selector,
                    span: span(),
                }),
            },
        ],
        result: selector,
        result_type: ValueType::Boolean,
        span: span(),
    };
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("switch selector is not i32")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_projection_that_is_not_dominated_by_its_tag_test() {
    // A field projected in the `case 1` arm must not be read in the sibling
    // `default` arm: neither arm dominates the other, so the value is not
    // available and the projection cannot be used there.
    let selector = ValueId(0);
    let projected = ValueId(1);
    let result = ValueId(2);
    let function = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "nondominating_projection".into(),
        parameters: vec![selector],
        values: vec![
            ValueDecl {
                id: selector,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: projected,
                ty: ValueType::I32,
            },
            ValueDecl {
                id: result,
                ty: ValueType::I32,
            },
        ],
        entry: BlockId(0),
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Switch {
                    value: selector,
                    cases: vec![(1, BlockId(1))],
                    default: BlockId(2),
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(1),
                parameters: Vec::new(),
                instructions: vec![Instruction::Constant {
                    destination: projected,
                    value: 7,
                    span: span(),
                }],
                terminator: Some(Terminator::Return {
                    value: projected,
                    span: span(),
                }),
            },
            BasicBlock {
                id: BlockId(2),
                parameters: Vec::new(),
                instructions: vec![Instruction::Primitive {
                    destination: result,
                    op: NumericOp::I32Add,
                    left: projected,
                    right: projected,
                    span: span(),
                }],
                terminator: Some(Terminator::Return {
                    value: result,
                    span: span(),
                }),
            },
        ],
        result,
        result_type: ValueType::I32,
        span: span(),
    };
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("outside its dominance scope")),
        "{errors:?}"
    );
}
