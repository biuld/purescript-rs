use super::Instruction;
use crate::cc::{Assignment, AssignmentKind, Function as CcFunction, Module as CcModule};
use crate::cc::{Reference, Representation, RepresentationTable, ValueShape};
use crate::types::ValueId;
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use std::sync::atomic::{AtomicU32, Ordering};

mod array_clone;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn next_artifact_id() -> u32 {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn wasi() -> crate::abi::WasiRegistry {
    crate::abi::WasiRegistry::load().expect("the vendored WASI WIT should load")
}

fn run_linear(module: CcModule, expected: &str) {
    let target = crate::TargetCapabilities {
        bulk_memory: true,
        ..crate::TargetCapabilities::wasm_mvp()
    };
    let (mir, _) = crate::mir::lower_module_with_capabilities(module, target)
        .expect("the CC module should lower without GC");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut wasi(), target)
        .expect("the linear MIR should lower to MVP Wasm");
    let binary = crate::wasm::encode_module(&wasm).expect("encoding linear MVP Wasm");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the linear MVP module should validate");
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok()
    {
        let path = std::env::temp_dir().join(format!(
            "psrs-linear-{}-{}-{}.wasm",
            expected,
            std::process::id(),
            next_artifact_id()
        ));
        std::fs::write(&path, &binary).expect("writing linear MVP Wasm");
        let output = std::process::Command::new("wasmtime")
            .arg("run")
            .arg("--invoke")
            .arg(crate::abi::RUN_CORE_EXPORT)
            .arg(&path)
            .output()
            .expect("running linear MVP Wasm");
        let _ = std::fs::remove_file(&path);
        assert!(output.status.success(), "wasmtime failed: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
    }
}

fn run_gc(mir: &crate::mir::Module, expected_code: i32) {
    let target = crate::TargetCapabilities::default();
    let mut wasi = wasi();
    let wasm = crate::wasm::lower_module_with_capabilities(mir, &mut wasi, target)
        .expect("the GC MIR should lower to Wasm");
    let core = crate::wasm::encode_module(&wasm).expect("encoding GC Wasm");
    crate::validator_for(target)
        .validate_all(&core)
        .expect("the GC module should validate");
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok()
    {
        let (resolve, world) = crate::component::command_world().expect("WASI WIT should load");
        let component = crate::component::componentize(&core, &resolve, world)
            .expect("componentizing the GC module");
        let path = std::env::temp_dir().join(format!("psrs-gc-linear-{}.wasm", std::process::id()));
        std::fs::write(&path, component).expect("writing GC component");
        let output = std::process::Command::new("wasmtime")
            .arg("run")
            .arg(&path)
            .output()
            .expect("running GC component");
        let _ = std::fs::remove_file(&path);
        assert_eq!(output.status.code(), Some(expected_code));
    }
}

#[test]
fn lowers_the_same_cc_product_through_the_linear_memory_planner() {
    let mut representations = RepresentationTable::default();
    let product = representations.reserve();
    representations.set(
        product,
        Representation::Product {
            fields: vec![ValueShape::Integer, ValueShape::Number],
        },
    );
    let symbol = SymbolId::new(ModuleId(0), 0);
    let module = CcModule {
        name: "LinearProduct".into(),
        externals: Vec::new(),
        representations,
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                crate::cc::ValueDecl {
                    id: ValueId(0),
                    ty: ValueShape::Integer,
                },
                crate::cc::ValueDecl {
                    id: ValueId(1),
                    ty: ValueShape::Number,
                },
                crate::cc::ValueDecl {
                    id: ValueId(2),
                    ty: ValueShape::Reference(Reference {
                        nullable: false,
                        heap: crate::cc::RefShape::Repr(product),
                    }),
                },
                crate::cc::ValueDecl {
                    id: ValueId(3),
                    ty: ValueShape::Integer,
                },
            ],
            assignments: vec![
                Assignment {
                    destination: ValueId(0),
                    kind: AssignmentKind::Constant(7),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::NumberConstant("2.5".into()),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::ProductNew {
                        destination: ValueId(2),
                        representation: product,
                        arguments: vec![ValueId(0), ValueId(1)],
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(3),
                    kind: AssignmentKind::ProductGet {
                        destination: ValueId(3),
                        representation: product,
                        field: 0,
                        value: ValueId(2),
                    },
                    span: span(),
                },
            ],
            result: ValueId(3),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };
    let target = crate::TargetCapabilities::wasm_mvp();
    let (gc_mir, _) = crate::mir::lower_module_with_capabilities(
        module.clone(),
        crate::TargetCapabilities::default(),
    )
    .expect("the same product should lower through the GC planner");
    assert!(!gc_mir.types.is_empty());
    let (mir, _) = crate::mir::lower_module_with_capabilities(module.clone(), target)
        .expect("the product should lower without GC");
    assert!(
        mir.functions[0].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::LinearAlloc { .. }))
    );
    assert!(
        mir.functions[0].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::LinearLoad { .. }))
    );
    assert_eq!(
        mir.functions[0].result_type,
        gc_mir.functions[0].result_type
    );
    run_gc(&gc_mir, 7);
    run_linear(module, "7");
}

#[test]
fn lowers_linear_arrays_with_pointer_arithmetic() {
    let mut representations = RepresentationTable::default();
    let array = representations.reserve();
    representations.set(
        array,
        Representation::Array {
            element: ValueShape::Integer,
        },
    );
    let symbol = SymbolId::new(ModuleId(0), 0);
    let module = CcModule {
        name: "LinearArray".into(),
        externals: Vec::new(),
        representations,
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                crate::cc::ValueDecl {
                    id: ValueId(0),
                    ty: ValueShape::Integer,
                },
                crate::cc::ValueDecl {
                    id: ValueId(1),
                    ty: ValueShape::Reference(Reference {
                        nullable: false,
                        heap: crate::cc::RefShape::Repr(array),
                    }),
                },
                crate::cc::ValueDecl {
                    id: ValueId(2),
                    ty: ValueShape::Integer,
                },
                crate::cc::ValueDecl {
                    id: ValueId(3),
                    ty: ValueShape::Integer,
                },
            ],
            assignments: vec![
                Assignment {
                    destination: ValueId(0),
                    kind: AssignmentKind::Constant(4),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::Constant(0),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::ArrayNew {
                        destination: ValueId(1),
                        representation: array,
                        elements: vec![ValueId(0), ValueId(0)],
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(3),
                    kind: AssignmentKind::ArrayGet {
                        destination: ValueId(3),
                        representation: array,
                        value: ValueId(1),
                        index: ValueId(2),
                    },
                    span: span(),
                },
            ],
            result: ValueId(3),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };
    run_linear(module, "4");
}

#[test]
fn lowers_linear_closures_through_the_function_table() {
    let signature = crate::cc::SignatureId(0);
    let caller_symbol = SymbolId::new(ModuleId(0), 0);
    let target_symbol = SymbolId::new(ModuleId(0), 1);
    let closure = ValueShape::Reference(Reference {
        nullable: false,
        heap: crate::cc::RefShape::Closure(signature),
    });
    let target = CcFunction {
        symbol: target_symbol,
        name: "target".into(),
        parameters: vec![ValueId(0)],
        values: vec![
            crate::cc::ValueDecl {
                id: ValueId(0),
                ty: ValueShape::Reference(Reference {
                    nullable: false,
                    heap: crate::cc::RefShape::Aggregate,
                }),
            },
            crate::cc::ValueDecl {
                id: ValueId(1),
                ty: ValueShape::Number,
            },
            crate::cc::ValueDecl {
                id: ValueId(2),
                ty: ValueShape::Integer,
            },
        ],
        assignments: vec![
            Assignment {
                destination: ValueId(1),
                kind: AssignmentKind::ClosureGetCapture {
                    closure: ValueId(0),
                    index: 0,
                },
                span: span(),
            },
            Assignment {
                destination: ValueId(2),
                kind: AssignmentKind::Constant(1),
                span: span(),
            },
        ],
        result: ValueId(2),
        result_type: ValueShape::Integer,
        span: span(),
    };
    let caller = CcFunction {
        symbol: caller_symbol,
        name: "caller".into(),
        parameters: Vec::new(),
        values: vec![
            crate::cc::ValueDecl {
                id: ValueId(0),
                ty: closure,
            },
            crate::cc::ValueDecl {
                id: ValueId(1),
                ty: ValueShape::Number,
            },
            crate::cc::ValueDecl {
                id: ValueId(2),
                ty: ValueShape::Integer,
            },
        ],
        assignments: vec![
            Assignment {
                destination: ValueId(1),
                kind: AssignmentKind::NumberConstant("2.5".into()),
                span: span(),
            },
            Assignment {
                destination: ValueId(0),
                kind: AssignmentKind::FunctionRef {
                    function: target_symbol,
                    signature,
                    captures: vec![ValueId(1)],
                },
                span: span(),
            },
            Assignment {
                destination: ValueId(2),
                kind: AssignmentKind::IndirectCall {
                    function: ValueId(0),
                    signature,
                    arguments: Vec::new(),
                },
                span: span(),
            },
        ],
        result: ValueId(2),
        result_type: ValueShape::Integer,
        span: span(),
    };
    let mut representations = RepresentationTable::default();
    representations.add_signature(crate::cc::Signature {
        parameters: Vec::new(),
        result: ValueShape::Integer,
    });
    let module = CcModule {
        name: "LinearClosure".into(),
        externals: Vec::new(),
        representations,
        functions: vec![caller, target],
        entry: Some(caller_symbol),
        span: span(),
    };
    let target = crate::TargetCapabilities::wasm_mvp();
    let (mir, _) = crate::mir::lower_module_with_capabilities(module.clone(), target)
        .expect("linear closure lowering should produce MIR");
    assert_eq!(mir.types.len(), 1);
    assert!(
        mir.functions[0].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::LinearClosureNew { .. }))
    );
    assert!(
        mir.functions[0].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::LinearClosureCall { .. }))
    );
    assert!(
        mir.functions[1].blocks[0]
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, Instruction::LinearClosureGetCapture { .. }))
    );
    run_linear(module, "1");
}
