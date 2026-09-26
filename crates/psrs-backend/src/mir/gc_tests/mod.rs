use super::Instruction;
use crate::cc::{Assignment, AssignmentKind, Function as CcFunction, Module as CcModule};
use crate::cc::{Reference, Representation, RepresentationTable, ValueShape};
use crate::types::ValueId;
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use std::sync::atomic::{AtomicU32, Ordering};

mod array;
mod binary_matrix;
mod div_mod;
mod erased;
mod scalar;
mod unary;
mod variant;

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

fn run_gc(mir: &crate::mir::Module, expected_code: i32) {
    if let Some(output) = run_gc_output(mir) {
        assert_eq!(output.status.code(), Some(expected_code));
    }
}

/// Lowers, validates, componentizes, and executes a MIR module, returning the
/// Wasmtime output, or `None` when Wasmtime is unavailable and not required.
fn run_gc_output(mir: &crate::mir::Module) -> Option<std::process::Output> {
    let target = crate::TargetCapabilities::default();
    let mut wasi = wasi();
    let wasm = crate::wasm::lower_module_with_capabilities(mir, &mut wasi, target)
        .expect("the GC MIR should lower to Wasm");
    let core = crate::wasm::encode_module(&wasm).expect("encoding GC Wasm");
    crate::validator_for(target)
        .validate_all(&core)
        .expect("the GC module should validate");
    let version = std::process::Command::new("wasmtime")
        .arg("--version")
        .output();
    let required = std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1");
    let Ok(version) = version else {
        if required {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
        }
        eprintln!("skipping: wasmtime is not installed");
        return None;
    };
    if !version.status.success() {
        if required {
            panic!("PSRS_REQUIRE_WASMTIME=1 but `wasmtime --version` failed: {version:?}");
        }
        eprintln!("skipping: wasmtime is unusable");
        return None;
    }
    let (resolve, world) = crate::component::command_world().expect("WASI WIT should load");
    let component = crate::component::componentize(&core, &resolve, world)
        .expect("componentizing the GC module");
    let path = std::env::temp_dir().join(format!(
        "psrs-gc-{}-{}.wasm",
        std::process::id(),
        next_artifact_id()
    ));
    std::fs::write(&path, component).expect("writing GC component");
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("running GC component");
    let _ = std::fs::remove_file(&path);
    Some(output)
}

#[test]
fn optimized_and_unoptimized_arithmetic_agree_on_an_oracle() {
    let symbol = SymbolId::new(ModuleId(0), 0);
    let mut values = Vec::new();
    for id in 0..6u32 {
        values.push(crate::cc::ValueDecl {
            id: ValueId(id),
            ty: ValueShape::Integer,
        });
    }
    let binary = |destination: u32, op: crate::cc::BinaryOp, left: u32, right: u32| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Primitive {
            op,
            left: ValueId(left),
            right: ValueId(right),
        },
        span: span(),
    };
    let constant = |destination: u32, value: i32| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Constant(value),
        span: span(),
    };
    let module = CcModule {
        name: "ArithmeticDifferential".into(),
        externals: Vec::new(),
        representations: RepresentationTable::default(),
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments: vec![
                constant(0, 7),
                constant(1, 5),
                binary(2, crate::cc::BinaryOp::IntAdd, 0, 1),
                constant(3, 3),
                binary(4, crate::cc::BinaryOp::IntMul, 2, 3),
                binary(5, crate::cc::BinaryOp::IntSub, 4, 2),
            ],
            result: ValueId(5),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };
    let target = crate::TargetCapabilities::default();
    let (mir, _) = crate::mir::lower_module_with_capabilities(module, target)
        .expect("arithmetic should lower to MIR");
    // `(7 + 5) * 3 - (7 + 5)` with wrapping i32 semantics.
    let oracle = ((7i32.wrapping_add(5)).wrapping_mul(3)).wrapping_sub(7i32.wrapping_add(5));
    run_gc(&mir, oracle);
    let optimized = crate::mir::opt::optimize(mir, target).expect("optimization should succeed");
    run_gc(&optimized, oracle);
}

#[test]
fn optimized_and_unoptimized_traps_agree() {
    // `1 / 0` must trap in both pipelines: the optimizer may not fold a
    // division whose divisor proves it will trap.
    let symbol = SymbolId::new(ModuleId(0), 0);
    let values = (0..3u32)
        .map(|id| crate::cc::ValueDecl {
            id: ValueId(id),
            ty: ValueShape::Integer,
        })
        .collect();
    let module = CcModule {
        name: "ArithmeticTrap".into(),
        externals: Vec::new(),
        representations: RepresentationTable::default(),
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments: vec![
                Assignment {
                    destination: ValueId(0),
                    kind: AssignmentKind::Constant(1),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::Constant(0),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::Primitive {
                        op: crate::cc::BinaryOp::IntQuot,
                        left: ValueId(0),
                        right: ValueId(1),
                    },
                    span: span(),
                },
            ],
            result: ValueId(2),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };
    let target = crate::TargetCapabilities::default();
    let (mir, _) = crate::mir::lower_module_with_capabilities(module, target)
        .expect("the division should lower to MIR");
    let optimized =
        crate::mir::opt::optimize(mir.clone(), target).expect("optimization should succeed");
    for candidate in [&mir, &optimized] {
        let Some(output) = run_gc_output(candidate) else {
            return;
        };
        assert!(
            !output.status.success(),
            "a division by zero must trap: {output:?}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("divide by zero"),
            "{output:?}"
        );
    }
}

#[test]
fn lowers_a_cc_product_through_the_gc_planner() {
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
        name: "GcProduct".into(),
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
    let (mir, _) =
        crate::mir::lower_module_with_capabilities(module, crate::TargetCapabilities::default())
            .expect("the product should lower through the GC planner");
    assert!(!mir.types.is_empty());
    run_gc(&mir, 7);
}
