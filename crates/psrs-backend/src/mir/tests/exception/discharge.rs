//! EH-09: the P9 discharge of a `psrs:runtime-exception` provider call into
//! the inline try region. The fixture lowers a hand-built CC module with the
//! checked `psrs:runtime-exception#checkThrows` binding through
//! `lower_module_with_bindings`, inspects the produced MIR, and executes the
//! artifact under wasmtime with an explicitly enabled target.

use super::*;
use crate::TargetCapabilities;
use crate::bindings::RuntimeBinding;
use crate::bindings::{CHECK_THROWS_EXPORT, ExternalBindings, RUNTIME_EXCEPTION_MODULE};
use crate::cc::{
    External, Function as CcFunction, Module as CcModule, RefShape, Reference, Signature,
    UnaryOp as CcUnaryOp,
};
use crate::mir::verify_module_with_capabilities;
use psrs_hir::FOREIGN_SYMBOL_BASE;

fn enabled_target() -> TargetCapabilities {
    TargetCapabilities {
        exceptions: true,
        ..TargetCapabilities::default()
    }
}

/// The CC requirements of the fixture: one `checkThrows` provider binding and
/// one guarded callable `thrower :: captures -> Unit -> Int`.
fn module() -> (CcModule, ExternalBindings) {
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
    let thrower = SymbolId::new(ModuleId(0), FOREIGN_SYMBOL_BASE + 1);
    let span = at(0);
    let cc = CcModule {
        name: "CheckThrowsDischarge".into(),
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
                    crate::cc::Assignment {
                        destination: ValueId(0),
                        kind: crate::cc::AssignmentKind::FunctionRef {
                            function: thrower,
                            signature: callable,
                            captures: Vec::new(),
                        },
                        span,
                    },
                    crate::cc::Assignment {
                        destination: ValueId(1),
                        kind: crate::cc::AssignmentKind::DirectCall {
                            contract: None,
                            function: check_throws,
                            arguments: vec![ValueId(0)],
                        },
                        span,
                    },
                    crate::cc::Assignment {
                        destination: ValueId(2),
                        kind: crate::cc::AssignmentKind::Unary {
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
            CcFunction {
                symbol: thrower,
                name: "thrower".into(),
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
                ],
                assignments: vec![crate::cc::Assignment {
                    destination: ValueId(5),
                    kind: crate::cc::AssignmentKind::Constant(5),
                    span,
                }],
                result: ValueId(5),
                result_type: crate::cc::ValueShape::Integer,
                span,
            },
        ],
        entry: Some(check_throws),
        span,
    };
    let binding = RuntimeBinding {
        symbol: check_throws,
        source_module: ModuleId(0),
        module: RUNTIME_EXCEPTION_MODULE.into(),
        function: CHECK_THROWS_EXPORT.into(),
        type_id: None,
        // The binding's own span, distinct from the call site's: the
        // disabled-profile rejection must carry it.
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

/// Replaces the guarded callable's normal return with a payload-less channel
/// throw, so the discharged region's catch path runs.
fn throw_in_callee(mir: &mut crate::mir::Module) {
    mir.tags.push(Tag {
        name: "psrs:stack-exhaustion".into(),
        payload: Vec::new(),
        span: at(310),
    });
    let function = mir
        .functions
        .iter_mut()
        .find(|function| function.name == "thrower")
        .expect("the fixture defines the guarded callable");
    let entry = function.entry;
    let block = function
        .blocks
        .iter_mut()
        .find(|block| block.id == entry)
        .expect("the callable has its entry block");
    block.terminator = Some(Terminator::Throw {
        tag: TagId(0),
        arguments: Vec::new(),
        handler: None,
        span: at(311),
    });
}

fn run(mir: &crate::mir::Module) -> Option<std::process::Output> {
    if !super::execution::require_engine() {
        return None;
    }
    super::execution::run_output(mir, enabled_target())
}

#[test]
fn discharges_a_check_throws_call_into_the_inline_try_region() {
    let (cc, bindings) = module();
    let target = enabled_target();
    let (mir, _) =
        crate::mir::lower_module_with_bindings(cc, bindings, target).expect("the discharge lowers");
    let main = &mir.functions[0];
    // The entry block ends in the region's Try; the protected block applies
    // the callable; the handler supplies the caught result; the join returns
    // the Boolean through the assignment's destination parameter.
    let Terminator::Try {
        protected,
        catches,
        catch_all,
        span,
    } = main.blocks[0].terminator.as_ref().expect("terminated")
    else {
        panic!("the entry terminator is the discharged Try");
    };
    assert_eq!(*protected, BlockId(1));
    assert_eq!(*catches, Vec::<(TagId, BlockId)>::new());
    assert_eq!(*catch_all, Some(BlockId(2)));
    assert_eq!(*span, at(0));
    // Both paths join on the shared block whose parameter is the destination.
    let jump = |block: &crate::mir::BasicBlock| match &block.terminator {
        Some(Terminator::Jump {
            target, arguments, ..
        }) => (*target, arguments.clone()),
        other => panic!("expected a jump to the join, got {other:?}"),
    };
    let (protected_join, false_arguments) = jump(&main.blocks[1]);
    let (handler_join, true_arguments) = jump(&main.blocks[2]);
    assert_eq!(protected_join, handler_join);
    // The join's parameter is the assignment's destination itself (the
    // Boolean the call produces): both paths deliver their value into it.
    assert_eq!(main.blocks[3].parameters, vec![ValueId(1)]);
    assert_eq!(false_arguments.len(), 1);
    assert_eq!(true_arguments.len(), 1);
    assert!(matches!(
        main.blocks[1]
            .instructions
            .iter()
            .find(|instruction| matches!(instruction, Instruction::ClosureCall { .. })),
        Some(Instruction::ClosureCall { .. })
    ));
    // The guarded application is the protected body's only call; the guard's
    // frame stays armed around it.
    verify_module_with_capabilities(&mir, enabled_target())
        .expect("the discharged module verifies");
}

#[test]
fn the_discharged_catch_path_returns_the_handler_s_value() {
    let (cc, bindings) = module();
    let target = enabled_target();
    let (mut mir, _) =
        crate::mir::lower_module_with_bindings(cc, bindings, target).expect("the discharge lowers");
    throw_in_callee(&mut mir);
    verify_module_with_capabilities(&mir, enabled_target()).expect("the thrown-to module verifies");
    let Some(output) = run(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(1),
        "the catch path must report `true`: {output:?}"
    );
}

#[test]
fn the_discharged_normal_path_returns_the_protected_value() {
    let (cc, bindings) = module();
    let target = enabled_target();
    let (mir, _) =
        crate::mir::lower_module_with_bindings(cc, bindings, target).expect("the discharge lowers");
    let Some(output) = run(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(0),
        "the normal path must report `false`: {output:?}"
    );
}

#[test]
fn rejects_a_check_throws_call_when_the_profile_disables_exceptions() {
    let (cc, bindings) = module();
    let disabled = TargetCapabilities {
        exceptions: false,
        ..TargetCapabilities::default()
    };
    let Err(errors) = crate::mir::lower_module_with_bindings(cc, bindings, disabled) else {
        panic!("a disabled profile must reject the provider call");
    };
    assert!(
        errors
            .iter()
            .any(|error| error.span == at(300)
                && error.message.contains("exception-handling proposal")),
        "{errors:?}"
    );
}
