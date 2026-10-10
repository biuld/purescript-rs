use super::*;
use crate::cc::{Assignment, AssignmentKind as A, ValueDecl, ValueId, ValueShape};
use crate::mir::{BlockId, Terminator};
use psrs_hir::{ModuleId, SymbolId};

pub(super) fn source(flag: bool) -> cc::Module {
    let (source, _) = super::tests::fixture();
    let mut source = (*source).clone();
    source.externals.clear();
    let span = source.span;
    let step = source.functions[0].result_type;
    let callee = |symbol, value| cc::Function {
        symbol,
        name: format!("produce_{value}"),
        parameters: vec![ValueId(0)],
        values: vec![
            ValueDecl {
                id: ValueId(0),
                ty: ValueShape::State,
            },
            ValueDecl {
                id: ValueId(1),
                ty: ValueShape::Integer,
            },
            ValueDecl {
                id: ValueId(2),
                ty: step,
            },
        ],
        assignments: vec![
            Assignment {
                destination: ValueId(1),
                kind: A::Constant(value),
                span,
            },
            Assignment {
                destination: ValueId(2),
                kind: A::ProductNew {
                    destination: ValueId(2),
                    representation: cc::ReprId(0),
                    arguments: vec![ValueId(0), ValueId(1)],
                },
                span,
            },
        ],
        result: ValueId(2),
        result_type: step,
        span,
    };
    source
        .functions
        .push(callee(SymbolId::new(ModuleId(0), 1), 42));
    source
        .functions
        .push(callee(SymbolId::new(ModuleId(0), 6), 43));
    let call = |id, symbol| Assignment {
        destination: ValueId(id),
        kind: A::DirectCall {
            function: symbol,
            arguments: vec![ValueId(0)],
        },
        span,
    };
    source.functions[0].values = vec![
        ValueDecl {
            id: ValueId(0),
            ty: ValueShape::State,
        },
        ValueDecl {
            id: ValueId(4),
            ty: ValueShape::Boolean,
        },
        ValueDecl {
            id: ValueId(1),
            ty: step,
        },
        ValueDecl {
            id: ValueId(5),
            ty: step,
        },
        ValueDecl {
            id: ValueId(3),
            ty: step,
        },
    ];
    source.functions[0].assignments = vec![
        Assignment {
            destination: ValueId(4),
            kind: A::Constant(i32::from(flag)),
            span,
        },
        Assignment {
            destination: ValueId(3),
            kind: A::If {
                condition: ValueId(4),
                then_assignments: vec![call(1, SymbolId::new(ModuleId(0), 1))],
                then_value: ValueId(1),
                else_assignments: vec![call(5, SymbolId::new(ModuleId(0), 6))],
                else_value: ValueId(5),
            },
            span,
        },
    ];
    source
}

#[test]
fn state_branches_lower_with_logical_joins_and_payload_only_block_parameters() {
    let (mir, _) = crate::mir::lower_module(source(true)).unwrap();
    let function = &mir.functions[0];
    assert_eq!(function.state.as_ref().unwrap().graph().blocks.len(), 4);
    assert_eq!(function.blocks.len(), 4);
    assert_eq!(function.blocks[3].parameters, vec![ValueId(3)]);
    assert!(function.parameters.is_empty());
    crate::mir::verify_module(&mir).unwrap();
}

#[test]
fn state_branch_projection_rejects_changed_edges_and_calls_moved_across_choices() {
    let (mut mir, _) = crate::mir::lower_module(source(true)).unwrap();
    if let Some(Terminator::Branch {
        then_block,
        else_block,
        ..
    }) = &mut mir.functions[0].blocks[0].terminator
    {
        std::mem::swap(then_block, else_block);
    }
    assert!(
        crate::mir::verify_module(&mir)
            .unwrap_err()
            .iter()
            .any(|failure| failure.message.contains("branch or successor"))
    );
    let (mut mir, _) = crate::mir::lower_module(source(true)).unwrap();
    let instruction = mir.functions[0].blocks[1].instructions.remove(0);
    mir.functions[0].blocks[2]
        .instructions
        .insert(0, instruction);
    assert!(
        crate::mir::verify_module(&mir)
            .unwrap_err()
            .iter()
            .any(|failure| failure.message.contains("adds or loses an invocation"))
    );
    let (mut mir, _) = crate::mir::lower_module(source(true)).unwrap();
    if let Some(Terminator::Jump { target, .. }) = &mut mir.functions[0].blocks[1].terminator {
        *target = BlockId(2);
    }
    assert!(crate::mir::verify_module(&mir).is_err());
}

#[test]
fn nested_choices_project_source_and_mir_block_orders_explicitly() {
    let mut source = source(true);
    let span = source.span;
    let step = source.functions[0].result_type;
    source.functions[0].values.extend([
        ValueDecl {
            id: ValueId(7),
            ty: ValueShape::Boolean,
        },
        ValueDecl {
            id: ValueId(10),
            ty: step,
        },
        ValueDecl {
            id: ValueId(11),
            ty: step,
        },
    ]);
    if let A::If {
        then_assignments, ..
    } = &mut source.functions[0].assignments[1].kind
    {
        let call = |id, symbol| Assignment {
            destination: ValueId(id),
            kind: A::DirectCall {
                function: SymbolId::new(ModuleId(0), symbol),
                arguments: vec![ValueId(0)],
            },
            span,
        };
        *then_assignments = vec![
            Assignment {
                destination: ValueId(7),
                kind: A::Constant(0),
                span,
            },
            Assignment {
                destination: ValueId(1),
                kind: A::If {
                    condition: ValueId(7),
                    then_assignments: vec![call(10, 1)],
                    then_value: ValueId(10),
                    else_assignments: vec![call(11, 6)],
                    else_value: ValueId(11),
                },
                span,
            },
        ];
    }
    let (mir, _) = crate::mir::lower_module(source).unwrap();
    assert_eq!(
        mir.functions[0]
            .state
            .as_ref()
            .unwrap()
            .graph()
            .blocks
            .len(),
        7
    );
    assert_eq!(mir.functions[0].blocks.len(), 7);
    assert!(matches!(
        mir.functions[0].blocks[1].terminator,
        Some(Terminator::Branch {
            then_block: BlockId(4),
            else_block: BlockId(5),
            ..
        })
    ));
    crate::mir::verify_module(&mir).unwrap();
}

#[test]
fn a_trapping_state_arm_has_no_normal_edge_to_the_payload_join() {
    let mut source = source(false);
    if let A::If {
        then_assignments, ..
    } = &mut source.functions[0].assignments[1].kind
    {
        then_assignments[0].kind = A::Unreachable;
    }
    let (mut mir, _) = crate::mir::lower_module(source).unwrap();
    assert!(matches!(
        mir.functions[0].blocks[1].terminator,
        Some(Terminator::Trap { .. })
    ));
    crate::mir::verify_module(&mir).unwrap();
    mir.functions[0].blocks[1].terminator = Some(Terminator::Jump {
        target: BlockId(3),
        arguments: vec![ValueId(1)],
        span: mir.span,
    });
    assert!(crate::mir::verify_module(&mir).is_err());
}

#[test]
fn state_branch_components_execute_both_selected_payloads_after_optimization() {
    for (flag, trapping, expected) in [
        (true, false, Some(42)),
        (false, false, Some(43)),
        (false, true, Some(43)),
        (true, true, None),
    ] {
        let target = crate::TargetCapabilities::default();
        let mut source = source(flag);
        if trapping
            && let A::If {
                then_assignments, ..
            } = &mut source.functions[0].assignments[1].kind
        {
            then_assignments[0].kind = A::Unreachable;
        }
        let (mir, mut wasi) = crate::mir::lower_module(source).unwrap();
        let mir = crate::mir::opt::optimize(mir, target).unwrap();
        assert_eq!(mir.functions[0].blocks.len(), 4);
        let context = crate::linking::default_context().unwrap();
        let link = crate::linking::plan_for_module(&context, &mir, &mut wasi, target).unwrap();
        let wasm = crate::wasm::lower_module_with_plan(&mir, &mut wasi, target, &link).unwrap();
        let core = crate::wasm::encode_module(&wasm).unwrap();
        let component = crate::linking::compose(&link, &core, mir.span, None).unwrap();
        crate::validator_for(target)
            .validate_all(&component)
            .unwrap();
        if std::process::Command::new("wasmtime")
            .arg("--version")
            .output()
            .is_err()
        {
            assert!(
                std::env::var_os("PSRS_REQUIRE_WASMTIME").is_none(),
                "Wasmtime is required"
            );
            return;
        }
        let path = std::env::temp_dir().join(format!(
            "psrs-state-choice-{}-{flag}-{trapping}.wasm",
            std::process::id()
        ));
        std::fs::write(&path, component).unwrap();
        let output = std::process::Command::new("wasmtime")
            .arg("run")
            .arg(&path)
            .output()
            .unwrap();
        std::fs::remove_file(path).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        if let Some(expected) = expected {
            assert_eq!(output.status.code(), Some(expected), "{stderr}");
        } else {
            assert!(!output.status.success(), "a selected trap must fail");
            assert!(stderr.contains("unreachable"), "{stderr}");
        }
    }
}
