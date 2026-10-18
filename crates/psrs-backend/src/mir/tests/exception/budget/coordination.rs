//! EH-15: the budget/limit coordination fixture — the protocol that validates
//! the budget, weight, and headroom constants. With the engine stack limit
//! pinned, deep non-tail recursion under the `checkThrows` discharge yields
//! the compiler's stack-exhaustion exception and never the engine's
//! `call stack exhausted` trap. A trap here means the constants are wrong.

use super::*;
use crate::bindings::{CHECK_THROWS_EXPORT, ExternalBindings, RUNTIME_EXCEPTION_MODULE};
use crate::cc::{
    Assignment, AssignmentKind, BinaryOp, External, Function as CcFunction, Module as CcModule,
    RefShape, Reference, Signature, UnaryOp as CcUnaryOp,
};
use crate::mir::verify_module_with_capabilities;
use psrs_hir::FOREIGN_SYMBOL_BASE;

fn enabled_target() -> TargetCapabilities {
    TargetCapabilities {
        exceptions: true,
        ..TargetCapabilities::default()
    }
}

/// Builds the CC module: `main` discharges `checkThrows` around a callable
/// that applies the genuinely non-tail self-recursive `rec` at `depth`.
pub(super) fn module(depth: i32) -> (CcModule, ExternalBindings) {
    let mut representations = crate::cc::RepresentationTable::default();
    let callable = representations.add_signature(Signature {
        parameters: vec![crate::cc::ValueShape::Integer],
        result: crate::cc::ValueShape::Integer,
    });
    let callable_shape = crate::cc::ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(callable),
    });
    let receiver = crate::cc::ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    });
    let check_throws = SymbolId::new(ModuleId(0), FOREIGN_SYMBOL_BASE);
    let apply = SymbolId::new(ModuleId(0), FOREIGN_SYMBOL_BASE + 1);
    let rec = SymbolId::new(ModuleId(0), FOREIGN_SYMBOL_BASE + 2);
    let span = at(0);
    let cc = CcModule {
        name: "StackExhaustionCoordination".into(),
        externals: vec![External {
            projection: None,
            symbol: check_throws,
            signature: Some(Signature {
                parameters: vec![callable_shape],
                result: crate::cc::ValueShape::Boolean,
            }),
        }],
        representations,
        functions: vec![
            // main: the guard around the deep call; its Int result is the
            // process exit code (0 = returned normally, 1 = caught).
            CcFunction {
                symbol: check_throws,
                name: "main".into(),
                parameters: Vec::new(),
                values: vec![
                    crate::cc::ValueDecl {
                        id: ValueId(0),
                        ty: callable_shape,
                    },
                    crate::cc::ValueDecl {
                        id: ValueId(1),
                        ty: crate::cc::ValueShape::Boolean,
                    },
                    crate::cc::ValueDecl {
                        id: ValueId(2),
                        ty: crate::cc::ValueShape::Integer,
                    },
                ],
                assignments: vec![
                    Assignment {
                        destination: ValueId(0),
                        kind: AssignmentKind::FunctionRef {
                            function: apply,
                            signature: callable,
                            captures: Vec::new(),
                        },
                        span,
                    },
                    Assignment {
                        destination: ValueId(1),
                        kind: AssignmentKind::DirectCall {
                            contract: None,
                            function: check_throws,
                            arguments: vec![ValueId(0)],
                        },
                        span,
                    },
                    Assignment {
                        destination: ValueId(2),
                        kind: AssignmentKind::Unary {
                            op: CcUnaryOp::BooleanToInt,
                            value: ValueId(1),
                        },
                        span,
                    },
                ],
                result: ValueId(2),
                result_type: crate::cc::ValueShape::Integer,
                span,
            },
            // The guarded callable: applies `rec` at the fixed depth.
            CcFunction {
                symbol: apply,
                name: "apply".into(),
                parameters: vec![ValueId(3), ValueId(4)],
                values: vec![
                    crate::cc::ValueDecl {
                        id: ValueId(3),
                        ty: receiver,
                    },
                    crate::cc::ValueDecl {
                        id: ValueId(4),
                        ty: crate::cc::ValueShape::Integer,
                    },
                    crate::cc::ValueDecl {
                        id: ValueId(5),
                        ty: crate::cc::ValueShape::Integer,
                    },
                    crate::cc::ValueDecl {
                        id: ValueId(6),
                        ty: crate::cc::ValueShape::Integer,
                    },
                ],
                assignments: vec![
                    Assignment {
                        destination: ValueId(5),
                        kind: AssignmentKind::Constant(depth),
                        span,
                    },
                    Assignment {
                        destination: ValueId(6),
                        kind: AssignmentKind::DirectCall {
                            contract: None,
                            function: rec,
                            arguments: vec![ValueId(5)],
                        },
                        span,
                    },
                ],
                result: ValueId(6),
                result_type: crate::cc::ValueShape::Integer,
                span,
            },
            // `rec(n) = if n <= 0 then 0 else 1 + rec(n - 1)`: the `1 +`
            // after the self call forces a real frame per level, so P9's
            // tail marking cannot convert it and every level consumes both
            // the estimated weight and the real native stack.
            CcFunction {
                symbol: rec,
                name: "rec".into(),
                parameters: vec![ValueId(10)],
                values: (10..=17)
                    .map(|id| crate::cc::ValueDecl {
                        id: ValueId(id),
                        ty: if id == 12 {
                            crate::cc::ValueShape::Boolean
                        } else {
                            crate::cc::ValueShape::Integer
                        },
                    })
                    .collect(),
                assignments: vec![
                    Assignment {
                        destination: ValueId(11),
                        kind: AssignmentKind::Constant(0),
                        span,
                    },
                    Assignment {
                        destination: ValueId(12),
                        kind: AssignmentKind::Primitive {
                            op: BinaryOp::IntLe,
                            left: ValueId(10),
                            right: ValueId(11),
                        },
                        span,
                    },
                    Assignment {
                        destination: ValueId(17),
                        kind: AssignmentKind::If {
                            condition: ValueId(12),
                            then_assignments: Vec::new(),
                            then_value: ValueId(11),
                            else_assignments: vec![
                                Assignment {
                                    destination: ValueId(13),
                                    kind: AssignmentKind::Constant(1),
                                    span,
                                },
                                Assignment {
                                    destination: ValueId(14),
                                    kind: AssignmentKind::Primitive {
                                        op: BinaryOp::IntSub,
                                        left: ValueId(10),
                                        right: ValueId(13),
                                    },
                                    span,
                                },
                                Assignment {
                                    destination: ValueId(15),
                                    kind: AssignmentKind::DirectCall {
                                        contract: None,
                                        function: rec,
                                        arguments: vec![ValueId(14)],
                                    },
                                    span,
                                },
                                Assignment {
                                    destination: ValueId(16),
                                    kind: AssignmentKind::Primitive {
                                        op: BinaryOp::IntAdd,
                                        left: ValueId(15),
                                        right: ValueId(13),
                                    },
                                    span,
                                },
                            ],
                            else_value: ValueId(16),
                        },
                        span,
                    },
                ],
                result: ValueId(17),
                result_type: crate::cc::ValueShape::Integer,
                span,
            },
        ],
        entry: Some(check_throws),
        span,
    };
    let binding = crate::bindings::RuntimeBinding {
        symbol: check_throws,
        source_module: ModuleId(0),
        module: RUNTIME_EXCEPTION_MODULE.into(),
        function: CHECK_THROWS_EXPORT.into(),
        type_id: None,
        span: at(300),
    };
    (
        cc,
        ExternalBindings {
            imports: Vec::new(),
            runtime: vec![binding],
        },
    )
}

fn run(depth: i32) -> Option<std::process::Output> {
    if !super::execution::require_engine() {
        return None;
    }
    let (cc, bindings) = module(depth);
    let (mir, _) = crate::mir::lower_module_with_bindings(cc, bindings, enabled_target())
        .expect("the discharge lowers");
    verify_module_with_capabilities(&mir, enabled_target()).expect("the module verifies");
    super::execution::run_output(&mir, enabled_target())
}

/// The coordination invariant: deep non-tail recursion under the discharged
/// `checkThrows` yields the compiler's stack-exhaustion exception — the guard
/// catches it and the program exits with the handler's value — never the
/// engine's `call stack exhausted` trap.
#[test]
fn deep_non_tail_recursion_yields_the_compiler_s_exception_never_a_trap() {
    let Some(output) = run(100_000) else {
        return;
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the guard must catch the compiler's stack-exhaustion throw: {output:?}"
    );
    assert!(
        !stderr.contains("call stack exhausted"),
        "the engine's trap must never surface: {output:?}"
    );
    assert!(
        !stderr.contains("unreachable"),
        "no guest trap may surface either: {output:?}"
    );
}

/// The same program at the corpus's returning depth (TCOMutRec's depth-100
/// shape): the recursion must return and `checkThrows` must report false —
/// the budget must not fire grossly early.
#[test]
fn depth_100_recursion_still_returns_under_the_pinned_budget() {
    let Some(output) = run(100) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "the depth-100 recursion must return normally: {output:?}"
    );
}
