//! Negative tail-call verifier fixtures: a `ReturnCall`/`ReturnCallRef` is
//! rejected unless its callee signature and result agree with its caller.

use super::verify_module;
use crate::mir::{BasicBlock, BlockId, Function, Instruction, Module, Terminator};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn symbol(index: u32) -> SymbolId {
    SymbolId::new(ModuleId(0), index)
}

fn module_with(caller: Function, callee: Function) -> Module {
    Module {
        name: "TailVerifierTest".into(),
        types: Vec::new(),
        strings: Vec::new(),
        imports: Vec::new(),
        entry: Some(caller.symbol),
        functions: vec![caller, callee],
        span: span(),
    }
}

fn callee(parameters: Vec<ValueDecl>, result_type: ValueType) -> Function {
    if let Some(parameter) = parameters.first() {
        let result = parameter.id;
        let parameter_ty = parameter.ty;
        return Function {
            id: FunctionId(1),
            symbol: symbol(1),
            name: "callee".into(),
            parameters: parameters.iter().map(|value| value.id).collect(),
            values: parameters,
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Some(Terminator::Return {
                    value: result,
                    span: span(),
                }),
            }],
            result,
            result_type: parameter_ty,
            span: span(),
        };
    }
    let result = ValueId(0);
    let instruction = match result_type {
        ValueType::F64 => Instruction::NumberConstant {
            destination: result,
            value: "1.0".into(),
            span: span(),
        },
        _ => Instruction::Constant {
            destination: result,
            value: 1,
            span: span(),
        },
    };
    Function {
        id: FunctionId(1),
        symbol: symbol(1),
        name: "callee".into(),
        parameters: Vec::new(),
        values: vec![ValueDecl {
            id: result,
            ty: result_type,
        }],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![instruction],
            terminator: Some(Terminator::Return {
                value: result,
                span: span(),
            }),
        }],
        result,
        result_type,
        span: span(),
    }
}

fn caller(
    result_type: ValueType,
    values: Vec<ValueDecl>,
    instructions: Vec<Instruction>,
    terminator: Terminator,
) -> Function {
    Function {
        id: FunctionId(0),
        symbol: symbol(0),
        name: "caller".into(),
        parameters: Vec::new(),
        values,
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions,
            terminator: Some(terminator),
        }],
        result: ValueId(99),
        result_type,
        span: span(),
    }
}

#[test]
fn rejects_a_direct_return_call_whose_result_differs_from_its_caller() {
    let callee = callee(Vec::new(), ValueType::F64);
    let caller = caller(
        ValueType::I32,
        Vec::new(),
        Vec::new(),
        Terminator::ReturnCall {
            function: symbol(1),
            arguments: Vec::new(),
            span: span(),
        },
    );
    let errors = verify_module(&module_with(caller, callee)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("differs from its caller result")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_direct_return_call_with_the_wrong_argument_type() {
    let callee = callee(
        vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
        }],
        ValueType::I32,
    );
    let caller = caller(
        ValueType::I32,
        vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::F64,
        }],
        vec![Instruction::NumberConstant {
            destination: ValueId(0),
            value: "1.0".into(),
            span: span(),
        }],
        Terminator::ReturnCall {
            function: symbol(1),
            arguments: vec![ValueId(0)],
            span: span(),
        },
    );
    let errors = verify_module(&module_with(caller, callee)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("tail call argument")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_return_call_ref_with_a_non_function_target() {
    let callee = callee(Vec::new(), ValueType::I32);
    let caller = caller(
        ValueType::I32,
        vec![ValueDecl {
            id: ValueId(0),
            ty: ValueType::I32,
        }],
        vec![Instruction::Constant {
            destination: ValueId(0),
            value: 0,
            span: span(),
        }],
        Terminator::ReturnCallRef {
            function: ValueId(0),
            arguments: Vec::new(),
            span: span(),
        },
    );
    let errors = verify_module(&module_with(caller, callee)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("call_ref target")),
        "{errors:?}"
    );
}
