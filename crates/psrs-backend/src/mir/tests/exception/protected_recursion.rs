//! A protected recursive activation must retain the caller's native guard.
use crate::{
    BackendInput, ExternalBindings, RuntimeBinding,
    cc::{self, Assignment, AssignmentKind as A, ValueShape as S},
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
fn a(id: u32, kind: A) -> Assignment {
    Assignment {
        destination: cc::ValueId(id),
        kind,
        span: TextRange::new(0, 1),
    }
}
fn fixture() -> BackendInput {
    let span = TextRange::new(0, 1);
    let main = SymbolId::new(ModuleId(0), 0);
    let rec = SymbolId::new(ModuleId(0), 1);
    let throw = SymbolId::new(ModuleId(0), psrs_hir::FOREIGN_SYMBOL_BASE);
    let call = |id, function, args: Vec<u32>| {
        a(
            id,
            A::DirectCall {
                contract: None,
                function,
                arguments: args.into_iter().map(cc::ValueId).collect(),
            },
        )
    };
    let values = |list: Vec<(u32, S)>| {
        list.into_iter()
            .map(|(id, ty)| cc::ValueDecl {
                id: cc::ValueId(id),
                ty,
            })
            .collect()
    };
    let body = cc::Region {
        assignments: vec![a(
            6,
            A::If {
                condition: cc::ValueId(2),
                then_assignments: vec![call(7, throw, vec![5])],
                then_value: cc::ValueId(7),
                else_assignments: vec![call(8, rec, vec![4])],
                else_value: cc::ValueId(8),
            },
        )],
        exit: cc::RegionExit::Yield(cc::ValueId(6)),
    };
    let handler = cc::Region {
        assignments: vec![a(
            10,
            A::If {
                condition: cc::ValueId(2),
                then_assignments: vec![call(13, throw, vec![5])],
                then_value: cc::ValueId(13),
                else_assignments: vec![a(12, A::Constant(42))],
                else_value: cc::ValueId(12),
            },
        )],
        exit: cc::RegionExit::Yield(cc::ValueId(10)),
    };
    let f = cc::Function {
        symbol: rec,
        name: "recursive_guard".into(),
        parameters: vec![cc::ValueId(0)],
        values: values(
            (0..14)
                .map(|i| {
                    (
                        i,
                        match i {
                            2 => S::Boolean,
                            5 => S::String,
                            11 => S::Reference(cc::Reference {
                                nullable: false,
                                heap: cc::RefShape::Exception,
                            }),
                            _ => S::Integer,
                        },
                    )
                })
                .collect(),
        ),
        assignments: vec![
            a(1, A::Constant(0)),
            a(
                2,
                A::Primitive {
                    op: cc::BinaryOp::IntEq,
                    left: cc::ValueId(0),
                    right: cc::ValueId(1),
                },
            ),
            a(3, A::Constant(1)),
            a(
                4,
                A::Primitive {
                    op: cc::BinaryOp::IntSub,
                    left: cc::ValueId(0),
                    right: cc::ValueId(3),
                },
            ),
            a(5, A::StringConstant("message".into())),
            a(
                9,
                A::Protect {
                    body,
                    exception: cc::ValueId(11),
                    handler,
                },
            ),
        ],
        result: cc::ValueId(9),
        result_type: S::Integer,
        span,
    };
    let entry = cc::Function {
        symbol: main,
        name: "main".into(),
        parameters: vec![],
        values: values(vec![(0, S::Integer), (1, S::Integer)]),
        assignments: vec![a(0, A::Constant(1)), call(1, rec, vec![0])],
        result: cc::ValueId(1),
        result_type: S::Integer,
        span,
    };
    let module = cc::Module {
        name: "ProtectedSelfCall".into(),
        externals: vec![cc::External {
            projection: None,
            symbol: throw,
            signature: Some(cc::Signature {
                parameters: vec![S::String],
                result: S::Integer,
            }),
        }],
        representations: Default::default(),
        functions: vec![entry, f],
        entry: Some(main),
        span,
    };
    BackendInput {
        cc: module,
        externals: ExternalBindings {
            imports: vec![],
            runtime: vec![RuntimeBinding {
                symbol: throw,
                source_module: ModuleId(0),
                module: "psrs:runtime-exception".into(),
                function: "throw".into(),
                type_id: None,
                span,
            }],
        },
        warnings: vec![],
    }
}

#[test]
fn protected_self_recursion_retains_dynamic_catches_on_every_profile() {
    for tail_call in [false, true] {
        let target = crate::TargetCapabilities {
            tail_call,
            ..super::enabled_target()
        };
        let input = fixture();
        let (mir, _) = crate::mir::lower_module_with_bindings(input.cc, input.externals, target)
            .expect("valid recursive protection must lower without a back edge to Try");
        let f = mir
            .functions
            .iter()
            .find(|f| f.name == "recursive_guard")
            .unwrap();
        assert!(
            f.blocks.iter().flat_map(|b| &b.instructions).any(
                |i| matches!(i,crate::mir::Instruction::Call { function,.. } if *function==f.symbol)
            ),
            "the guarded self-call retains its activation"
        );
        if !super::execution::require_engine() {
            return;
        }
        let optimized = crate::mir::opt::optimize(mir.clone(), target).unwrap();
        for artifact in [mir, optimized] {
            let output = super::execution::run_output(&artifact, target).unwrap();
            // f(0) catches and rethrows; f(1)'s older guard must still catch.
            assert_eq!(
                output.status.code(),
                Some(42),
                "tail_call={tail_call}: {output:?}"
            );
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
        }
    }
}
