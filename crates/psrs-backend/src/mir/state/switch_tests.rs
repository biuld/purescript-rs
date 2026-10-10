use crate::cc::{self, Assignment, AssignmentKind as A, TagCase, ValueDecl, ValueId, ValueShape};
use crate::mir::{BlockId, Terminator};
use psrs_hir::{ModuleId, SymbolId};

fn source(tag: i32) -> cc::Module {
    let mut source = super::branch_tests::source(true);
    let span = source.span;
    let mut default = source.functions[1].clone();
    default.symbol = SymbolId::new(ModuleId(0), 8);
    default.assignments[0].kind = A::Constant(44);
    source.functions.push(default);
    let function = &mut source.functions[0];
    function
        .values
        .iter_mut()
        .find(|value| value.id == ValueId(4))
        .unwrap()
        .ty = ValueShape::Integer;
    function.values.push(ValueDecl {
        id: ValueId(7),
        ty: function.result_type,
    });
    function.assignments[0].kind = A::Constant(tag);
    let call = |id, symbol| Assignment {
        destination: ValueId(id),
        kind: A::DirectCall {
            function: SymbolId::new(ModuleId(0), symbol),
            arguments: vec![ValueId(0)],
        },
        span,
    };
    function.assignments[1].kind = A::TagSwitch {
        value: ValueId(4),
        cases: vec![
            TagCase {
                tag: 7,
                assignments: vec![call(1, 1)],
                value: ValueId(1),
            },
            TagCase {
                tag: -3,
                assignments: vec![call(5, 6)],
                value: ValueId(5),
            },
        ],
        default_assignments: vec![call(7, 8)],
        default_value: ValueId(7),
    };
    source
}

#[test]
fn multiway_state_projection_checks_labels_default_edges_and_invocation_placement() {
    let (module, _) = crate::mir::lower_module(source(7)).unwrap();
    crate::mir::verify_module(&module).unwrap();
    assert_eq!(module.functions[0].blocks.len(), 5);
    for mutation in 0..6 {
        let mut changed = module.clone();
        if mutation < 4 {
            let Some(Terminator::Switch {
                value,
                cases,
                default,
                ..
            }) = &mut changed.functions[0].blocks[0].terminator
            else {
                panic!()
            };
            match mutation {
                0 => cases[0].0 = 9,
                1 => cases[0].1 = BlockId(2),
                2 => *default = BlockId(1),
                _ => *value = ValueId(3),
            }
        } else if mutation == 4 {
            let call = changed.functions[0].blocks[1].instructions.remove(0);
            changed.functions[0].blocks[2].instructions.insert(0, call);
        } else {
            let Some(Terminator::Jump { arguments, .. }) =
                &mut changed.functions[0].blocks[1].terminator
            else {
                panic!()
            };
            arguments[0] = ValueId(4);
        }
        let errors = crate::mir::verify_module(&changed).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.pass == "P9 MIR dependency verification"),
            "{mutation}: {errors:?}"
        );
    }
}

#[test]
fn nested_binary_choice_in_a_state_switch_preserves_both_join_payloads() {
    let mut source = source(7);
    let function = &mut source.functions[0];
    let span = function.span;
    function.values.extend([
        ValueDecl {
            id: ValueId(8),
            ty: ValueShape::Boolean,
        },
        ValueDecl {
            id: ValueId(10),
            ty: function.result_type,
        },
        ValueDecl {
            id: ValueId(11),
            ty: function.result_type,
        },
    ]);
    let A::TagSwitch { cases, .. } = &mut function.assignments[1].kind else {
        panic!()
    };
    let call = |id, symbol| Assignment {
        destination: ValueId(id),
        kind: A::DirectCall {
            function: SymbolId::new(ModuleId(0), symbol),
            arguments: vec![ValueId(0)],
        },
        span,
    };
    cases[0].assignments = vec![
        Assignment {
            destination: ValueId(8),
            kind: A::Constant(1),
            span,
        },
        Assignment {
            destination: ValueId(1),
            kind: A::If {
                condition: ValueId(8),
                then_assignments: vec![call(10, 1)],
                then_value: ValueId(10),
                else_assignments: vec![call(11, 6)],
                else_value: ValueId(11),
            },
            span,
        },
    ];
    let (module, _) = crate::mir::lower_module(source).unwrap();
    assert_eq!(module.functions[0].blocks.len(), 8);
    crate::mir::verify_module(&module).unwrap();
    let mut changed = module;
    let Some(Terminator::Jump { arguments, .. }) = &mut changed.functions[0].blocks[7].terminator
    else {
        panic!()
    };
    arguments[0] = ValueId(4);
    assert!(
        crate::mir::verify_module(&changed)
            .unwrap_err()
            .iter()
            .any(|error| error.pass == "P9 MIR dependency verification")
    );
}

#[test]
fn state_switch_executes_case_default_and_selected_trap_after_optimization() {
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
    for trapping in [false, true] {
        for (tag, expected) in [(7, 42), (-3, 43), (99, 44)] {
            let target = crate::TargetCapabilities::default();
            let mut source = source(tag);
            if trapping {
                let A::TagSwitch { cases, .. } = &mut source.functions[0].assignments[1].kind
                else {
                    panic!()
                };
                cases[0].assignments[0].kind = A::Unreachable;
            }
            let (module, mut wasi) = crate::mir::lower_module(source).unwrap();
            let module = crate::mir::opt::optimize(module, target).unwrap();
            let context = crate::linking::default_context().unwrap();
            let link =
                crate::linking::plan_for_module(&context, &module, &mut wasi, target).unwrap();
            let wasm =
                crate::wasm::lower_module_with_plan(&module, &mut wasi, target, &link).unwrap();
            let core = crate::wasm::encode_module(&wasm).unwrap();
            let component = crate::linking::compose(&link, &core, module.span, None).unwrap();
            let path = std::env::temp_dir().join(format!(
                "psrs-state-switch-{}-{tag}-{trapping}.wasm",
                std::process::id()
            ));
            std::fs::write(&path, component).unwrap();
            let output = std::process::Command::new("wasmtime")
                .arg("run")
                .arg(&path)
                .output()
                .unwrap();
            std::fs::remove_file(path).unwrap();
            if trapping && tag == 7 {
                assert!(!output.status.success());
                assert!(String::from_utf8_lossy(&output.stderr).contains("unreachable"));
            } else {
                assert_eq!(output.status.code(), Some(expected), "{output:?}");
            }
        }
    }
}

#[test]
fn constant_payload_joins_keep_their_source_parameters_during_optimization() {
    let mut source = source(7);
    let function = &mut source.functions[0];
    let span = function.span;
    let arms = |destination, payload| {
        vec![
            Assignment {
                destination: ValueId(payload),
                kind: A::Constant(42),
                span,
            },
            Assignment {
                destination: ValueId(destination),
                kind: A::ProductNew {
                    destination: ValueId(destination),
                    representation: cc::ReprId(0),
                    arguments: vec![ValueId(0), ValueId(payload)],
                },
                span,
            },
        ]
    };
    for payload in [10, 11, 12] {
        function.values.push(ValueDecl {
            id: ValueId(payload),
            ty: ValueShape::Integer,
        });
    }
    let A::TagSwitch {
        cases,
        default_assignments,
        ..
    } = &mut function.assignments[1].kind
    else {
        panic!()
    };
    cases[0].assignments = arms(1, 10);
    cases[1].assignments = arms(5, 11);
    *default_assignments = arms(7, 12);
    let (module, _) = crate::mir::lower_module(source).unwrap();
    let optimized =
        crate::mir::opt::optimize(module, crate::TargetCapabilities::default()).unwrap();
    assert_eq!(optimized.functions[0].blocks[4].parameters, [ValueId(3)]);
    crate::mir::verify_module(&optimized).unwrap();
}

#[test]
fn raw_state_switch_results_reject_before_their_effectful_arms_can_be_erased() {
    let mut source = source(7);
    source.functions.truncate(1);
    source.externals = [1, 6, 8]
        .into_iter()
        .map(|symbol| cc::External {
            symbol: SymbolId::new(ModuleId(0), symbol),
            signature: Some(cc::Signature {
                parameters: vec![ValueShape::State],
                result: source.functions[0].result_type,
            }),
            projection: None,
        })
        .collect();
    let function = &mut source.functions[0];
    let span = function.span;
    function.values.extend((10..=13).map(|id| ValueDecl {
        id: ValueId(id),
        ty: ValueShape::State,
    }));
    let choice = &mut function.assignments[1];
    choice.destination = ValueId(13);
    let A::TagSwitch {
        cases,
        default_assignments,
        default_value,
        ..
    } = &mut choice.kind
    else {
        panic!()
    };
    let projection = |destination, value| Assignment {
        destination: ValueId(destination),
        kind: A::ProductGet {
            destination: ValueId(destination),
            representation: cc::ReprId(0),
            field: 0,
            value: ValueId(value),
        },
        span,
    };
    for (index, case) in cases.iter_mut().enumerate() {
        let state = 10 + index as u32;
        case.assignments.push(projection(state, case.value.0));
        case.value = ValueId(state);
    }
    default_assignments.push(projection(12, default_value.0));
    *default_value = ValueId(12);
    function.assignments.push(Assignment {
        destination: ValueId(3),
        kind: A::DirectCall {
            function: SymbolId::new(ModuleId(0), 1),
            arguments: vec![ValueId(13)],
        },
        span,
    });
    cc::state::check(&source).unwrap();
    let Err(errors) = super::project(source, &[]) else {
        panic!("raw State choice must reject");
    };
    assert!(
        errors
            .iter()
            .any(|error| error.pass == "P9 State projection"
                && error.message.contains("raw State choice")),
        "{errors:?}"
    );
}
