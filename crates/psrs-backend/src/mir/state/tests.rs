use super::*;
use crate::cc::{Assignment, AssignmentKind, ReprId, Representation, Signature, ValueShape};
use crate::mir::{BasicBlock, BlockId, Function, Terminator};
use crate::types::{FunctionId, ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

pub(super) fn fixture() -> (Arc<cc::Module>, Function) {
    let span = TextRange::new(0, 1);
    let owner = SymbolId::new(ModuleId(0), 0);
    let callee = SymbolId::new(ModuleId(0), 1);
    let step = ValueShape::Reference(cc::Reference {
        nullable: false,
        heap: cc::RefShape::Repr(ReprId(0)),
    });
    let mut representations = cc::RepresentationTable::default();
    representations
        .representations
        .push(Representation::Product {
            fields: vec![ValueShape::State, ValueShape::Integer],
        });
    let source = cc::Module {
        name: "DependencyProjection".into(),
        representations,
        externals: vec![cc::External {
            symbol: callee,
            signature: Some(Signature {
                parameters: vec![ValueShape::State],
                result: step,
            }),
            projection: None,
        }],
        functions: vec![cc::Function {
            symbol: owner,
            name: "action".into(),
            parameters: vec![ValueId(0)],
            values: [ValueShape::State, step, ValueShape::State, step]
                .into_iter()
                .enumerate()
                .map(|(id, ty)| cc::ValueDecl {
                    id: ValueId(id as u32),
                    ty,
                })
                .collect(),
            assignments: vec![
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::DirectCall {
                        function: callee,
                        arguments: vec![ValueId(0)],
                    },
                    span,
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::ProductGet {
                        destination: ValueId(2),
                        representation: ReprId(0),
                        field: 0,
                        value: ValueId(1),
                    },
                    span,
                },
                Assignment {
                    destination: ValueId(3),
                    kind: AssignmentKind::DirectCall {
                        function: callee,
                        arguments: vec![ValueId(2)],
                    },
                    span,
                },
            ],
            result: ValueId(3),
            result_type: step,
            span,
        }],
        entry: Some(owner),
        span,
    };
    let function = Function {
        state: None,
        id: FunctionId(0),
        symbol: owner,
        name: "action".into(),
        parameters: Vec::new(),
        values: vec![
            ValueDecl {
                id: ValueId(1),
                ty: ValueType::I32,
            },
            ValueDecl {
                id: ValueId(3),
                ty: ValueType::I32,
            },
        ],
        entry: BlockId(0),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            parameters: Vec::new(),
            instructions: vec![
                Instruction::Call {
                    destination: ValueId(1),
                    function: callee,
                    arguments: Vec::new(),
                    span,
                },
                Instruction::Call {
                    destination: ValueId(3),
                    function: callee,
                    arguments: Vec::new(),
                    span,
                },
            ],
            terminator: Some(Terminator::Return {
                value: ValueId(3),
                span,
            }),
        }],
        result: ValueId(3),
        result_type: ValueType::I32,
        span,
    };
    (Arc::new(source), function)
}

#[test]
fn checked_projection_retains_zero_width_dependencies_at_actual_calls() {
    let (source, function) = fixture();
    let flow = DependencyFlow::checked(source, &function).unwrap();
    assert_eq!(flow.graph().blocks[0].transitions.len(), 2);
    assert!(function.parameters.is_empty());
    assert_eq!(function.values.len(), 2);
    flow.verify(&function).unwrap();
}

#[test]
fn source_certificate_cannot_replace_instruction_correspondence() {
    let (source, mut function) = fixture();
    let flow = DependencyFlow::checked(source, &function).unwrap();
    function.blocks[0].instructions.swap(0, 1);
    assert!(flow.verify(&function).is_err());
    function.blocks[0].instructions.pop();
    assert!(flow.verify(&function).is_err());
}

#[test]
fn changing_call_producer_or_normal_successor_rejects() {
    let (source, mut function) = fixture();
    let flow = DependencyFlow::checked(source, &function).unwrap();
    let original = function.blocks[0].instructions[0].clone();
    function.blocks[0].instructions[0] = Instruction::Constant {
        destination: ValueId(1),
        value: 0,
        span: function.span,
    };
    assert!(flow.verify(&function).is_err());
    function.blocks[0].instructions[0] = original;
    function.blocks[0].terminator = None;
    assert!(flow.verify(&function).is_err());
}

fn physical_module(source: Arc<cc::Module>, mut function: Function) -> super::super::Module {
    function.state = Some(DependencyFlow::checked(source.clone(), &function).unwrap());
    super::super::Module {
        name: "DependencyProjection".into(),
        types: Vec::new(),
        strings: Vec::new(),
        imports: vec![super::super::Import {
            runtime: None,
            symbol: SymbolId::new(ModuleId(0), 1),
            parameters: Vec::new(),
            result: Some(ValueType::I32),
        }],
        entry: Some(function.symbol),
        span: function.span,
        functions: vec![function],
        dependencies: Inventory::checked(source).unwrap(),
        layout: None,
    }
}

#[test]
fn mir_verifier_rejects_extra_invocations_against_the_source_body() {
    let (source, function) = fixture();
    let mut module = physical_module(source, function);
    super::super::verify_module(&module).unwrap();
    let first = module.functions[0].blocks[0].instructions[0].clone();
    module.functions[0].blocks[0].instructions.insert(0, first);
    let failures = super::super::verify_module(&module).unwrap_err();
    assert!(
        failures
            .iter()
            .any(|failure| failure.message.contains("adds or loses an invocation"))
    );
}

#[test]
fn dependency_evidence_from_another_source_cannot_replace_the_required_witness() {
    let (source, function) = fixture();
    let mut other = (*source).clone();
    other.name = "OtherSource".into();
    let replacement = DependencyFlow::checked(Arc::new(other), &function).unwrap();
    let mut module = physical_module(source, function);
    module.functions[0].state = Some(replacement);
    let errors = super::super::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| { error.message.contains("different source inventory") })
    );
}

#[test]
fn pruning_an_unused_dependency_function_preserves_requirements_for_live_functions() {
    let (source, function) = fixture();
    let mut source = (*source).clone();
    let mut unused_source = source.functions[0].clone();
    unused_source.symbol = SymbolId::new(ModuleId(0), 2);
    source.functions.push(unused_source);
    let source = Arc::new(source);
    let mut unused = function.clone();
    unused.id = FunctionId(1);
    unused.symbol = source.functions[1].symbol;
    unused.state = Some(DependencyFlow::checked(source.clone(), &unused).unwrap());
    let mut module = physical_module(source, function);
    module.functions.push(unused);
    let mut optimized =
        super::super::opt::optimize(module, crate::TargetCapabilities::default()).unwrap();
    assert_eq!(optimized.functions.len(), 1);
    super::super::verify_module(&optimized).unwrap();
    optimized.functions[0].state = None;
    let errors = super::super::verify_module(&optimized).unwrap_err();
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("missing required dependency evidence")
    }));
}

#[test]
fn optimization_retains_an_invocation_with_an_unused_physical_payload() {
    let (source, function) = fixture();
    let module = physical_module(source, function);
    let optimized =
        super::super::opt::optimize(module, crate::TargetCapabilities::default()).unwrap();
    assert_eq!(optimized.functions[0].blocks[0].instructions.len(), 2);
    assert_eq!(
        optimized.functions[0]
            .state
            .as_ref()
            .unwrap()
            .graph()
            .blocks[0]
            .transitions
            .len(),
        2
    );
    super::super::verify_module(&optimized).unwrap();
}

#[test]
fn logical_state_cannot_be_reintroduced_as_a_physical_zero() {
    let (source, mut function) = fixture();
    let flow = DependencyFlow::checked(source, &function).unwrap();
    function.values.push(ValueDecl {
        id: ValueId(0),
        ty: ValueType::I32,
    });
    function.blocks[0].instructions.insert(
        0,
        Instruction::Constant {
            destination: ValueId(0),
            value: 0,
            span: function.span,
        },
    );
    assert!(flow.verify(&function).unwrap_err().iter().any(|failure| {
        failure
            .message
            .contains("materializes a logical State slot")
    }));
}

#[test]
fn trapping_source_requires_an_actual_trap_without_a_normal_successor() {
    let (mut source, mut function) = fixture();
    Arc::make_mut(&mut source).functions[0].assignments[2].kind = AssignmentKind::Unreachable;
    function.blocks[0].instructions[1] = Instruction::Unreachable {
        destination: ValueId(3),
        span: function.span,
    };
    function.blocks[0].terminator = Some(Terminator::Trap {
        span: function.span,
    });
    let flow = DependencyFlow::checked(source, &function).unwrap();
    function.blocks[0].instructions.pop();
    assert!(flow.verify(&function).is_err());
}

#[test]
fn ordinary_state_calls_lower_and_execute_with_only_physical_payloads() {
    let (source, _) = fixture();
    let mut source = (*source).clone();
    source.externals.clear();
    let span = source.span;
    let step = source.functions[0].result_type;
    source.functions.push(cc::Function {
        symbol: SymbolId::new(ModuleId(0), 1),
        name: "produce".into(),
        parameters: vec![ValueId(0)],
        values: vec![
            cc::ValueDecl {
                id: ValueId(0),
                ty: ValueShape::State,
            },
            cc::ValueDecl {
                id: ValueId(1),
                ty: ValueShape::Integer,
            },
            cc::ValueDecl {
                id: ValueId(2),
                ty: step,
            },
        ],
        assignments: vec![
            Assignment {
                destination: ValueId(1),
                kind: AssignmentKind::Constant(42),
                span,
            },
            Assignment {
                destination: ValueId(2),
                kind: AssignmentKind::ProductNew {
                    destination: ValueId(2),
                    representation: ReprId(0),
                    arguments: vec![ValueId(0), ValueId(1)],
                },
                span,
            },
        ],
        result: ValueId(2),
        result_type: step,
        span,
    });
    let target = crate::TargetCapabilities::default();
    let (mir, mut wasi) = super::super::lower_module(source).unwrap();
    assert!(mir.functions.iter().all(|function| function.state.is_some()
        && function.parameters.is_empty()
        && function.result_type == ValueType::I32));
    let mir = super::super::opt::optimize(mir, target).unwrap();
    assert_eq!(
        mir.functions[0].blocks[0]
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction, Instruction::Call { .. }))
            .count(),
        2
    );
    assert!(
        !mir.functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction, Instruction::StructNew { .. }))
    );
    let context = crate::linking::default_context().unwrap();
    let link = crate::linking::plan_for_module(&context, &mir, &mut wasi, target).unwrap();
    let wasm = crate::wasm::lower_module_with_plan(&mir, &mut wasi, target, &link).unwrap();
    let core = crate::wasm::encode_module(&wasm).unwrap();
    crate::validator_for(target).validate_all(&core).unwrap();
    let component = crate::linking::compose(&link, &core, span, None).unwrap();
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
    let path =
        std::env::temp_dir().join(format!("psrs-state-projection-{}.wasm", std::process::id()));
    std::fs::write(&path, component).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        output.status.code(),
        Some(42),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
