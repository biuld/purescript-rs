//! Structural CC-verifier fixtures: declaration order, definition/use order,
//! call arity, branch shapes, capture indices, and tag switches.

use super::super::{verify_function, verify_module};
use super::{symbol, table};
use crate::cc::{
    Assignment, AssignmentKind, BinaryOp, Function, RefShape, Reference, Signature, TagCase,
    ValueDecl, ValueId, ValueShape,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn range() -> TextRange {
    TextRange::new(0, 1)
}

fn integer(id: ValueId) -> ValueDecl {
    ValueDecl {
        id,
        ty: ValueShape::Integer,
    }
}

fn boolean(id: ValueId) -> ValueDecl {
    ValueDecl {
        id,
        ty: ValueShape::Boolean,
    }
}

fn aggregate(id: ValueId) -> ValueDecl {
    ValueDecl {
        id,
        ty: ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Aggregate,
        }),
    }
}

#[test]
fn accepts_parameters_that_are_the_first_value_declarations() {
    let parameter = ValueId(0);
    let result = ValueId(1);
    let function = Function {
        symbol: symbol(0),
        name: "valid".into(),
        parameters: vec![parameter],
        values: vec![integer(parameter), integer(result)],
        assignments: vec![Assignment {
            destination: result,
            kind: AssignmentKind::Primitive {
                op: BinaryOp::IntAdd,
                left: parameter,
                right: parameter,
            },
            span: range(),
        }],
        result,
        result_type: ValueShape::Integer,
        span: range(),
    };
    assert!(verify_function(&function, &std::collections::HashMap::new(), &table()).is_ok());
}

#[test]
fn rejects_parameters_that_are_not_the_first_value_declarations() {
    let first = ValueId(0);
    let parameter = ValueId(1);
    let function = Function {
        symbol: symbol(0),
        name: "misplaced".into(),
        parameters: vec![parameter],
        values: vec![integer(first), integer(parameter)],
        assignments: vec![Assignment {
            destination: first,
            kind: AssignmentKind::Constant(1),
            span: range(),
        }],
        result: parameter,
        result_type: ValueShape::Integer,
        span: range(),
    };
    let errors = verify_function(&function, &std::collections::HashMap::new(), &table())
        .expect_err("a parameter declared after another value must be rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("first value declarations"))
    );
    assert!(
        errors
            .iter()
            .all(|error| error.kind == crate::BackendErrorKind::InvalidCompilerIr),
        "a CC verifier failure is invalid compiler IR, not unsupported source"
    );
}

#[test]
fn rejects_a_value_used_before_its_assignment() {
    let parameter = ValueId(0);
    let later = ValueId(1);
    let result = ValueId(2);
    let function = Function {
        symbol: symbol(0),
        name: "forward_reference".into(),
        parameters: vec![parameter],
        values: vec![integer(parameter), integer(later), boolean(result)],
        assignments: vec![
            Assignment {
                destination: result,
                kind: AssignmentKind::Primitive {
                    op: BinaryOp::IntLt,
                    left: parameter,
                    right: later,
                },
                span: range(),
            },
            Assignment {
                destination: later,
                kind: AssignmentKind::Constant(1),
                span: range(),
            },
        ],
        result,
        result_type: ValueShape::Boolean,
        span: range(),
    };
    assert!(verify_function(&function, &std::collections::HashMap::new(), &table()).is_err());
}

#[test]
fn rejects_a_direct_call_with_the_wrong_arity() {
    let callee = symbol(1);
    let destination = ValueId(0);
    let function = Function {
        symbol: symbol(0),
        name: "bad_arity".into(),
        parameters: Vec::new(),
        values: vec![integer(destination)],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::DirectCall {
                function: callee,
                arguments: Vec::new(),
            },
            span: range(),
        }],
        result: destination,
        result_type: ValueShape::Integer,
        span: range(),
    };
    let signatures = std::collections::HashMap::from([(
        callee,
        Signature {
            parameters: vec![ValueShape::Integer],
            result: ValueShape::Integer,
        },
    )]);
    assert!(verify_function(&function, &signatures, &table()).is_err());
}

#[test]
fn rejects_if_branches_with_different_result_shapes() {
    let condition = ValueId(0);
    let then_value = ValueId(1);
    let else_value = ValueId(2);
    let destination = ValueId(3);
    let function = Function {
        symbol: symbol(0),
        name: "branch_shapes".into(),
        parameters: vec![condition],
        values: vec![
            boolean(condition),
            integer(then_value),
            ValueDecl {
                id: else_value,
                ty: ValueShape::Number,
            },
            integer(destination),
        ],
        assignments: vec![
            Assignment {
                destination: then_value,
                kind: AssignmentKind::Constant(1),
                span: range(),
            },
            Assignment {
                destination: else_value,
                kind: AssignmentKind::NumberConstant("1.0".into()),
                span: range(),
            },
            Assignment {
                destination,
                kind: AssignmentKind::If {
                    condition,
                    then_assignments: Vec::new(),
                    then_value,
                    else_assignments: Vec::new(),
                    else_value,
                },
                span: range(),
            },
        ],
        result: destination,
        result_type: ValueShape::Integer,
        span: range(),
    };
    assert!(verify_function(&function, &std::collections::HashMap::new(), &table()).is_err());
}

#[test]
fn rejects_non_contiguous_closure_capture_indices() {
    let closure = ValueId(0);
    let capture = ValueId(1);
    let function = Function {
        symbol: symbol(0),
        name: "gap_in_captures".into(),
        parameters: vec![closure],
        values: vec![aggregate(closure), integer(capture)],
        assignments: vec![Assignment {
            destination: capture,
            kind: AssignmentKind::ClosureGetCapture { closure, index: 1 },
            span: range(),
        }],
        result: capture,
        result_type: ValueShape::Integer,
        span: range(),
    };
    assert!(verify_function(&function, &std::collections::HashMap::new(), &table()).is_err());
}

#[test]
fn rejects_a_tag_switch_with_duplicate_tags() {
    let selector = ValueId(0);
    let default_value = ValueId(1);
    let result = ValueId(2);
    let case_value = ValueId(3);
    let function = Function {
        symbol: symbol(0),
        name: "duplicate_tags".into(),
        parameters: vec![selector, case_value],
        values: vec![
            integer(selector),
            integer(default_value),
            integer(result),
            integer(case_value),
        ],
        assignments: vec![
            Assignment {
                destination: default_value,
                kind: AssignmentKind::Constant(0),
                span: range(),
            },
            Assignment {
                destination: case_value,
                kind: AssignmentKind::Constant(1),
                span: range(),
            },
            Assignment {
                destination: result,
                kind: AssignmentKind::TagSwitch {
                    value: selector,
                    cases: vec![
                        TagCase {
                            tag: 0,
                            assignments: Vec::new(),
                            value: case_value,
                        },
                        TagCase {
                            tag: 0,
                            assignments: Vec::new(),
                            value: case_value,
                        },
                    ],
                    default_assignments: Vec::new(),
                    default_value,
                },
                span: range(),
            },
        ],
        result,
        result_type: ValueShape::Integer,
        span: range(),
    };
    assert!(verify_function(&function, &std::collections::HashMap::new(), &table()).is_err());
}

#[test]
fn rejects_an_external_that_conflicts_with_a_function_symbol() {
    let destination = ValueId(0);
    let function = Function {
        symbol: symbol(0),
        name: "conflict".into(),
        parameters: Vec::new(),
        values: vec![integer(destination)],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::Constant(0),
            span: range(),
        }],
        result: destination,
        result_type: ValueShape::Integer,
        span: range(),
    };
    let module = crate::cc::Module {
        name: "conflict".into(),
        externals: vec![crate::cc::External {
            symbol: symbol(0),
            signature: None,
        }],
        representations: table(),
        functions: vec![function],
        entry: None,
        span: range(),
    };
    assert!(verify_module(&module).is_err());
}

#[test]
fn rejects_a_function_symbol_defined_twice() {
    let function = |name: &str| {
        let destination = ValueId(0);
        Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: name.into(),
            parameters: Vec::new(),
            values: vec![integer(destination)],
            assignments: vec![Assignment {
                destination,
                kind: AssignmentKind::Constant(0),
                span: range(),
            }],
            result: destination,
            result_type: ValueShape::Integer,
            span: range(),
        }
    };
    let module = crate::cc::Module {
        name: "duplicate".into(),
        externals: Vec::new(),
        representations: table(),
        functions: vec![function("first"), function("second")],
        entry: None,
        span: range(),
    };
    assert!(verify_module(&module).is_err());
}
