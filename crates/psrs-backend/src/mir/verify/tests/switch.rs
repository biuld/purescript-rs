use super::{module_with_function, span, verify_module};
use crate::mir::{BasicBlock, BlockId, Function, Terminator};
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
