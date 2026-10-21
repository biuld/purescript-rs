//! Physical source-error contracts and real named String-payload recovery.
use super::*;
use crate::cc::{self, AssignmentKind as A, ValueShape as S};

fn assignment(destination: u32, kind: A) -> cc::Assignment {
    cc::Assignment {
        destination: ValueId(destination),
        kind,
        span: at(600),
    }
}
fn value(id: u32, ty: S) -> cc::ValueDecl {
    cc::ValueDecl {
        id: ValueId(id),
        ty,
    }
}
fn closure(signature: cc::SignatureId) -> S {
    S::Reference(cc::Reference {
        nullable: false,
        heap: cc::RefShape::Closure(signature),
    })
}
fn fixture() -> (cc::Module, crate::bindings::ExternalBindings) {
    let (mut cc, mut bindings) =
        super::throw_destination::fixture(super::throw_destination::Site::Tail, S::String);
    let body = cc::SignatureId(0);
    let handler = cc.representations.add_signature(cc::Signature {
        parameters: vec![S::String],
        result: S::String,
    });
    let receiver = S::Reference(cc::Reference {
        nullable: false,
        heap: cc::RefShape::Aggregate,
    });
    let recover = SymbolId::new(ModuleId(0), psrs_hir::FOREIGN_SYMBOL_BASE + 3);
    let catch = bindings.runtime[0].symbol;
    bindings.runtime[0].function = "catch".into();
    cc.externals[0].signature = Some(cc::Signature {
        parameters: vec![closure(body), closure(handler)],
        result: S::String,
    });
    let thrower = cc.functions[1].symbol;
    let main = &mut cc.functions[0];
    main.values = vec![
        value(0, closure(body)),
        value(1, closure(handler)),
        value(2, S::String),
        value(3, S::String),
        value(4, S::Boolean),
        value(5, S::Integer),
        value(6, S::Integer),
        value(7, S::Integer),
    ];
    main.result = ValueId(7);
    main.assignments = vec![
        assignment(
            0,
            A::FunctionRef {
                function: thrower,
                signature: body,
                captures: vec![],
            },
        ),
        assignment(
            1,
            A::FunctionRef {
                function: recover,
                signature: handler,
                captures: vec![],
            },
        ),
        assignment(
            2,
            A::DirectCall {
                contract: None,
                function: catch,
                arguments: vec![ValueId(0), ValueId(1)],
            },
        ),
        assignment(3, A::StringConstant("probe".into())),
        assignment(
            4,
            A::Primitive {
                op: cc::BinaryOp::StringEq,
                left: ValueId(2),
                right: ValueId(3),
            },
        ),
        assignment(
            5,
            A::Unary {
                op: cc::UnaryOp::BooleanToInt,
                value: ValueId(4),
            },
        ),
        assignment(6, A::Constant(42)),
        assignment(
            7,
            A::Primitive {
                op: cc::BinaryOp::IntMul,
                left: ValueId(5),
                right: ValueId(6),
            },
        ),
    ];
    cc.functions.push(cc::Function {
        symbol: recover,
        name: "recover".into(),
        parameters: vec![ValueId(0), ValueId(1)],
        values: vec![value(0, receiver), value(1, S::String)],
        assignments: vec![],
        result: ValueId(1),
        result_type: S::String,
        span: at(610),
    });
    (cc, bindings)
}

#[test]
fn named_catch_receives_the_real_string_payload_after_type_remapping() {
    let (cc, bindings) = fixture();
    let (mir, _) = crate::mir::lower_module_with_bindings(cc, bindings, enabled_target()).unwrap();
    assert_eq!(mir.tags.len(), 1);
    assert_eq!(mir.tags[0].name, crate::bindings::SOURCE_ERROR_TAG_NAME);
    assert!(matches!(
        mir.tags[0].payload.as_slice(),
        [ValueType::Ref(_)]
    ));
    assert!(mir.imports.is_empty(), "the provider has no host ABI");
    let main = &mir.functions[0];
    let Terminator::Try { clauses, .. } = terminator(&main.blocks[0]) else {
        panic!("named region")
    };
    assert!(matches!(
        clauses.as_slice(),
        [crate::mir::CatchClause::Catch { tag: TagId(0), .. }]
    ));
    if !super::execution::require_engine() {
        return;
    }
    let optimized = crate::mir::opt::optimize(mir.clone(), enabled_target()).unwrap();
    for artifact in [mir, optimized] {
        let output = super::execution::run_output(&artifact, enabled_target()).unwrap();
        assert_eq!(output.status.code(), Some(42), "{output:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
    }
}

#[test]
fn unused_throw_declarations_are_checked_without_planning_a_source_tag() {
    let (mut cc, bindings) =
        super::throw_destination::fixture(super::throw_destination::Site::Tail, S::String);
    cc.functions[1].assignments = vec![assignment(7, A::StringConstant("normal".into()))];
    let (mir, _) =
        crate::mir::lower_module_with_bindings(cc.clone(), bindings.clone(), enabled_target())
            .unwrap();
    assert!(
        mir.tags.is_empty(),
        "checkThrows plus an unused throw does not plan a tag"
    );
    cc.externals[1].signature.as_mut().unwrap().parameters[0] = S::Integer;
    assert!(crate::mir::lower_module_with_bindings(cc, bindings, enabled_target()).is_err());
}

#[test]
fn catch_rejects_mismatched_results_and_handler_operands_at_the_physical_boundary() {
    let (cc, bindings) = fixture();
    for change in 0..3 {
        let mut wrong = cc.clone();
        match change {
            0 => wrong.externals[0].signature.as_mut().unwrap().result = S::Integer,
            1 => wrong.representations.signatures[1].parameters[0] = S::Integer,
            _ => wrong.representations.signatures[1].result = S::Integer,
        }
        assert!(
            crate::mir::lower_module_with_bindings(wrong, bindings.clone(), enabled_target())
                .is_err()
        );
    }
    let disabled = crate::TargetCapabilities {
        exceptions: false,
        ..enabled_target()
    };
    let Err(errors) = crate::mir::lower_module_with_bindings(cc, bindings, disabled) else {
        panic!("disabled profile must reject source exception operations");
    };
    assert!(
        errors
            .iter()
            .any(|e| e.span == at(300) && e.message.contains("exception-handling proposal")),
        "{errors:?}"
    );
}
