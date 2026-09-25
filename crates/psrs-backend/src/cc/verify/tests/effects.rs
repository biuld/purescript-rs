//! CC-verifier fixtures for the `Effect` closure and trusted runner boundary.
//!
//! An effect is a closure from a hidden token to a result. These fixtures
//! reject malformed token positions, call shapes, and runner bindings before
//! the encoding stage.

use super::super::{verify_function, verify_module};
use super::{symbol, table};
use crate::cc::{
    Assignment, AssignmentKind, Function, RefShape, Reference, Signature, SignatureId, ValueDecl,
    ValueId, ValueShape,
};
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

fn aggregate(id: ValueId) -> ValueDecl {
    ValueDecl {
        id,
        ty: ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Aggregate,
        }),
    }
}

fn erased_shape() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

fn closure_shape(signature: SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

/// A closure whose hidden token is not the argument position the closure's
/// signature declares is malformed internal IR for an effect.
#[test]
fn rejects_an_effect_closure_with_the_token_in_the_wrong_position() {
    let signature = SignatureId(0);
    let closure_parameter = ValueId(0);
    let token = ValueId(1);
    let value = ValueId(2);
    let target = Function {
        symbol: symbol(1),
        name: "effect_target".into(),
        parameters: vec![closure_parameter, token, value],
        values: vec![
            aggregate(closure_parameter),
            integer(token),
            ValueDecl {
                id: value,
                ty: erased_shape(),
            },
        ],
        assignments: Vec::new(),
        result: value,
        result_type: erased_shape(),
        span: range(),
    };
    let caller_closure = ValueId(0);
    let function_value = ValueId(1);
    let caller = Function {
        symbol: symbol(0),
        name: "effect_caller".into(),
        parameters: vec![caller_closure],
        values: vec![
            aggregate(caller_closure),
            ValueDecl {
                id: function_value,
                ty: closure_shape(signature),
            },
        ],
        assignments: vec![Assignment {
            destination: function_value,
            kind: AssignmentKind::FunctionRef {
                function: symbol(1),
                signature,
                captures: Vec::new(),
            },
            span: range(),
        }],
        result: function_value,
        result_type: closure_shape(signature),
        span: range(),
    };
    let mut representations = table();
    representations.signatures.push(Signature {
        parameters: vec![erased_shape(), ValueShape::Integer],
        result: erased_shape(),
    });
    let module = crate::cc::Module {
        name: "wrong_token".into(),
        externals: Vec::new(),
        representations,
        functions: vec![caller, target],
        entry: None,
        span: range(),
    };
    let errors = verify_module(&module)
        .expect_err("a closure whose token is in the wrong position must be rejected");
    assert!(
        errors
            .iter()
            .all(|error| error.kind == crate::BackendErrorKind::InvalidCompilerIr),
        "{errors:?}"
    );
}

/// A closure call that produces the wrong result shape for the effect's
/// declared signature is rejected before encoding.
#[test]
fn rejects_an_effect_closure_call_with_the_wrong_result_shape() {
    let signature = SignatureId(0);
    let closure = ValueId(0);
    let token = ValueId(1);
    let destination = ValueId(2);
    let function = Function {
        symbol: symbol(0),
        name: "bad_effect_call".into(),
        parameters: vec![closure, token],
        values: vec![
            ValueDecl {
                id: closure,
                ty: closure_shape(signature),
            },
            integer(token),
            integer(destination),
        ],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::IndirectCall {
                function: closure,
                signature,
                arguments: vec![token],
            },
            span: range(),
        }],
        result: destination,
        result_type: ValueShape::Integer,
        span: range(),
    };
    let mut representations = table();
    representations.signatures.push(Signature {
        parameters: vec![ValueShape::Integer],
        result: erased_shape(),
    });
    let errors = verify_function(
        &function,
        &std::collections::HashMap::new(),
        &representations,
    )
    .expect_err("an effect closure call must produce its signature's result shape");
    assert!(
        errors
            .iter()
            .all(|error| error.kind == crate::BackendErrorKind::InvalidCompilerIr),
        "{errors:?}"
    );
}

/// A closure call that omits the hidden token has the wrong arity.
#[test]
fn rejects_an_effect_closure_call_with_the_wrong_arity() {
    let signature = SignatureId(0);
    let closure = ValueId(0);
    let token = ValueId(1);
    let destination = ValueId(2);
    let function = Function {
        symbol: symbol(0),
        name: "effect_call_without_token".into(),
        parameters: vec![closure, token],
        values: vec![
            ValueDecl {
                id: closure,
                ty: closure_shape(signature),
            },
            integer(token),
            ValueDecl {
                id: destination,
                ty: erased_shape(),
            },
        ],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::IndirectCall {
                function: closure,
                signature,
                arguments: Vec::new(),
            },
            span: range(),
        }],
        result: destination,
        result_type: erased_shape(),
        span: range(),
    };
    let mut representations = table();
    representations.signatures.push(Signature {
        parameters: vec![ValueShape::Integer],
        result: erased_shape(),
    });
    let errors = verify_function(
        &function,
        &std::collections::HashMap::new(),
        &representations,
    )
    .expect_err("an effect closure call must supply the token");
    assert!(
        errors
            .iter()
            .all(|error| error.kind == crate::BackendErrorKind::InvalidCompilerIr),
        "{errors:?}"
    );
}

/// A direct call to an unbound imported runner has no signature and is
/// malformed internal IR.
#[test]
fn rejects_a_direct_call_to_an_unbound_runner_external() {
    let runner = symbol(1);
    let destination = ValueId(0);
    let function = Function {
        symbol: symbol(0),
        name: "unauthorized_runner".into(),
        parameters: Vec::new(),
        values: vec![integer(destination)],
        assignments: vec![Assignment {
            destination,
            kind: AssignmentKind::DirectCall {
                function: runner,
                arguments: Vec::new(),
            },
            span: range(),
        }],
        result: destination,
        result_type: ValueShape::Integer,
        span: range(),
    };
    let module = crate::cc::Module {
        name: "unbound_runner".into(),
        externals: vec![crate::cc::External {
            symbol: runner,
            signature: None,
        }],
        representations: table(),
        functions: vec![function],
        entry: None,
        span: range(),
    };
    let errors =
        verify_module(&module).expect_err("an unbound runner external cannot be called directly");
    assert!(
        errors
            .iter()
            .all(|error| error.kind == crate::BackendErrorKind::InvalidCompilerIr),
        "{errors:?}"
    );
}
