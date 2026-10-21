//! Probe 1: a diverging provider must never define or read its destination.
//! Use the existing checkThrows fixture to observe throws as exit 42 and normal
//! returns as exit 0, through the real CC -> MIR -> component pipeline.
use super::*;
use crate::bindings::{RUNTIME_EXCEPTION_MODULE, RuntimeBinding};
use crate::cc::{self, Assignment, AssignmentKind as A, ValueShape as S};

#[derive(Clone, Copy, Debug)]
pub(super) enum Site {
    Tail,
    Suffix,
    If(bool),
    BothArms,
    Switch(i32),
    Protect(bool),
}

fn assignment(destination: u32, kind: A) -> Assignment {
    Assignment {
        destination: ValueId(destination),
        kind,
        span: at(420),
    }
}

pub(super) fn fixture(site: Site, result: S) -> (cc::Module, crate::bindings::ExternalBindings) {
    let (mut cc, mut bindings) = super::discharge::module();
    // Entry-level uncaught errors also exit 1. Scale checkThrows' Boolean to
    // 42 so execution proves the outer observer actually caught the throw.
    let main = &mut cc.functions[0];
    for id in [11, 12] {
        main.values.push(cc::ValueDecl {
            id: ValueId(id),
            ty: S::Integer,
        });
    }
    main.assignments.push(assignment(11, A::Constant(42)));
    main.assignments.push(assignment(
        12,
        A::Primitive {
            op: cc::BinaryOp::IntMul,
            left: ValueId(2),
            right: ValueId(11),
        },
    ));
    main.result = ValueId(12);
    let throw = SymbolId::new(ModuleId(0), psrs_hir::FOREIGN_SYMBOL_BASE + 2);
    cc.externals.push(cc::External {
        projection: None,
        symbol: throw,
        signature: Some(cc::Signature {
            parameters: vec![S::String],
            result,
        }),
    });
    bindings.runtime.push(RuntimeBinding {
        symbol: throw,
        source_module: ModuleId(0),
        module: RUNTIME_EXCEPTION_MODULE.into(),
        function: "throw".into(),
        type_id: None,
        span: at(400),
    });
    cc.representations.signatures[0].result = result;
    let f = &mut cc.functions[1];
    f.result_type = result;
    f.values.iter_mut().find(|v| v.id == ValueId(5)).unwrap().ty = result;
    for (id, ty) in [
        (6, S::String),
        (7, result),
        (8, S::Integer),
        (9, result),
        (
            10,
            S::Reference(cc::Reference {
                nullable: false,
                heap: cc::RefShape::Exception,
            }),
        ),
    ] {
        f.values.push(cc::ValueDecl {
            id: ValueId(id),
            ty,
        });
    }
    let raise = || {
        assignment(
            7,
            A::DirectCall {
                contract: None,
                function: throw,
                arguments: vec![ValueId(6)],
            },
        )
    };
    let normal = || {
        assignment(
            9,
            match result {
                S::String => A::StringConstant("normal".into()),
                _ => A::Constant(42),
            },
        )
    };
    f.assignments = vec![assignment(6, A::StringConstant("probe".into()))];
    match site {
        Site::Tail | Site::Suffix => {
            f.assignments.push(raise());
            f.result = ValueId(7);
            if matches!(site, Site::Suffix) {
                // This reachable-looking suffix must never be emitted after throw.
                f.assignments.push(normal());
                f.result = ValueId(9);
            }
        }
        Site::If(_) | Site::BothArms => {
            let selected = match site {
                Site::If(b) => b,
                _ => true,
            };
            f.assignments
                .push(assignment(8, A::Constant(i32::from(selected))));
            f.values.iter_mut().find(|v| v.id == ValueId(8)).unwrap().ty = S::Boolean;
            let (other, other_value) = if matches!(site, Site::BothArms) {
                (vec![assignment(9, raise().kind)], ValueId(9))
            } else {
                (vec![normal()], ValueId(9))
            };
            f.assignments.push(assignment(
                5,
                A::If {
                    condition: ValueId(8),
                    then_assignments: vec![raise()],
                    then_value: ValueId(7),
                    else_assignments: other,
                    else_value: other_value,
                },
            ));
        }
        Site::Switch(selected) => {
            f.assignments.push(assignment(8, A::Constant(selected)));
            f.assignments.push(assignment(
                5,
                A::TagSwitch {
                    value: ValueId(8),
                    cases: vec![cc::TagCase {
                        tag: 0,
                        assignments: vec![raise()],
                        value: ValueId(7),
                    }],
                    default_assignments: vec![normal()],
                    default_value: ValueId(9),
                },
            ));
        }
        Site::Protect(in_handler) => {
            let (body, handler) = if in_handler {
                // An inner protection catches a throw, then throws again from
                // its cleanup handler; the outer checkThrows must see it.
                (
                    cc::Region {
                        assignments: vec![assignment(9, raise().kind)],
                        exit: cc::RegionExit::Yield(ValueId(9)),
                    },
                    cc::Region {
                        assignments: vec![raise()],
                        exit: cc::RegionExit::Propagate(ValueId(10)),
                    },
                )
            } else {
                (
                    cc::Region {
                        assignments: vec![raise()],
                        exit: cc::RegionExit::Yield(ValueId(7)),
                    },
                    cc::Region {
                        assignments: vec![],
                        exit: cc::RegionExit::Propagate(ValueId(10)),
                    },
                )
            };
            f.assignments.push(assignment(
                5,
                A::Protect {
                    body,
                    exception: ValueId(10),
                    handler,
                },
            ));
        }
    }
    (cc, bindings)
}

#[test]
fn throw_destination_probe() {
    for result in [S::Integer, S::String] {
        for site in [
            Site::Tail,
            Site::Suffix,
            Site::If(true),
            Site::If(false),
            Site::BothArms,
            Site::Switch(0),
            Site::Switch(1),
            Site::Protect(false),
            Site::Protect(true),
        ] {
            let (cc, bindings) = fixture(site, result);
            let (mir, _) = crate::mir::lower_module_with_bindings(cc, bindings, enabled_target())
                .unwrap_or_else(|e| panic!("{site:?}/{result:?}: {e:?}"));
            crate::mir::verify_module_with_capabilities(&mir, enabled_target()).unwrap();
            let f = &mir.functions[1];
            // Throw's result declaration may remain, but no instruction may
            // manufacture it and no terminator may read it on the throw path.
            for block in &f.blocks {
                if matches!(block.terminator, Some(Terminator::Throw { .. })) {
                    assert!(
                        !block
                            .instructions
                            .iter()
                            .any(|i| i.destination() == Some(ValueId(7)))
                    );
                }
            }
            if !super::execution::require_engine() {
                return;
            }
            let optimized = crate::mir::opt::optimize(mir.clone(), enabled_target())
                .unwrap_or_else(|e| panic!("optimization {site:?}/{result:?}: {e:?}"));
            for (mode, artifact) in [("bypass", mir), ("optimized", optimized)] {
                let output = super::execution::run_output(&artifact, enabled_target()).unwrap();
                let expected = if matches!(site, Site::If(false) | Site::Switch(1)) {
                    0
                } else {
                    42
                };
                assert_eq!(
                    output.status.code(),
                    Some(expected),
                    "{mode}/{site:?}/{result:?}: {output:?}"
                );
                assert!(output.stdout.is_empty(), "{output:?}");
                assert!(output.stderr.is_empty(), "{output:?}");
                eprintln!("{mode}/{site:?}/{result:?}: compile verified, runtime exit {expected}");
            }
        }
    }
}
