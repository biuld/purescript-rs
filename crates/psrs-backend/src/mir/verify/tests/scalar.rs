use super::*;

fn primitive_function(
    left_ty: ValueType,
    right_ty: ValueType,
    result_ty: ValueType,
    op: crate::mir::NumericOp,
) -> Function {
    let left = ValueId(0);
    let right = ValueId(1);
    let output = ValueId(2);
    Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "bad_primitive".into(),
        parameters: vec![left, right],
        values: vec![
            ValueDecl {
                id: left,
                ty: left_ty,
            },
            ValueDecl {
                id: right,
                ty: right_ty,
            },
            ValueDecl {
                id: output,
                ty: result_ty,
            },
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::Primitive {
                destination: output,
                op,
                left,
                right,
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: output,
                span: span(),
            }),
        }],
        result: output,
        result_type: result_ty,
        span: span(),
    }
}

fn expect_primitive_rejection(
    left_ty: ValueType,
    right_ty: ValueType,
    result_ty: ValueType,
    op: crate::mir::NumericOp,
) {
    let function = primitive_function(left_ty, right_ty, result_ty, op);
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("primitive operand or result type")),
        "{errors:?}"
    );
}

#[test]
fn rejects_i32_arithmetic_on_f64_operands() {
    expect_primitive_rejection(
        ValueType::F64,
        ValueType::F64,
        ValueType::I32,
        crate::mir::NumericOp::I32Add,
    );
}

#[test]
fn rejects_f64_arithmetic_on_i32_operands() {
    expect_primitive_rejection(
        ValueType::I32,
        ValueType::I32,
        ValueType::F64,
        crate::mir::NumericOp::F64Add,
    );
}

#[test]
fn rejects_boolean_logic_on_i32_operands() {
    expect_primitive_rejection(
        ValueType::I32,
        ValueType::I32,
        ValueType::Boolean,
        crate::mir::NumericOp::BoolAnd,
    );
}

#[test]
fn rejects_an_integer_comparison_that_produces_i32() {
    expect_primitive_rejection(
        ValueType::I32,
        ValueType::I32,
        ValueType::I32,
        crate::mir::NumericOp::I32LtS,
    );
}

#[test]
fn rejects_a_boolean_constant_that_is_not_canonical() {
    let output = ValueId(0);
    let function = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "bad_boolean_constant".into(),
        parameters: Vec::new(),
        values: vec![ValueDecl {
            id: output,
            ty: ValueType::Boolean,
        }],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::Constant {
                destination: output,
                value: 2,
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: output,
                span: span(),
            }),
        }],
        result: output,
        result_type: ValueType::Boolean,
        span: span(),
    };
    let errors = verify_module(&module_with_function(function, Vec::new())).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("integer constant")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_call_with_the_wrong_argument_type() {
    let callee_symbol = SymbolId::new(ModuleId(0), 1);
    let helper = Function {
        id: crate::types::FunctionId(1),
        symbol: callee_symbol,
        name: "__psrs_euclidean_int_div".into(),
        parameters: vec![ValueId(0), ValueId(1)],
        values: vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(1),
                ty: ValueType::I32,
            },
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Some(Terminator::Return {
                value: ValueId(0),
                span: span(),
            }),
        }],
        result: ValueId(0),
        result_type: ValueType::I32,
        span: span(),
    };
    let argument = ValueId(0);
    let call_result = ValueId(1);
    let caller = Function {
        id: crate::types::FunctionId(0),
        symbol: SymbolId::new(ModuleId(0), 0),
        name: "caller".into(),
        parameters: vec![argument],
        values: vec![
            ValueDecl {
                id: argument,
                ty: ValueType::F64,
            },
            ValueDecl {
                id: call_result,
                ty: ValueType::I32,
            },
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![Instruction::Call {
                destination: call_result,
                function: callee_symbol,
                arguments: vec![argument, argument],
                span: span(),
            }],
            terminator: Some(Terminator::Return {
                value: call_result,
                span: span(),
            }),
        }],
        result: call_result,
        result_type: ValueType::I32,
        span: span(),
    };
    let mut module = module_with_function(caller, Vec::new());
    module.functions.push(helper);
    let errors = verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("call argument")),
        "{errors:?}"
    );
}
