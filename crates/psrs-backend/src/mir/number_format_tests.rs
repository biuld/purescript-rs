//! End-to-end formatter artifact execution through the checked plan.

use super::{BasicBlock, BlockId, Function, Import, Instruction, Module, Terminator};
use crate::types::{ValueDecl, ValueId, ValueType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn registry() -> crate::abi::WasiRegistry {
    crate::abi::WasiRegistry::load().expect("the vendored WASI WIT should load")
}

/// A MIR function that allocates a bounded buffer, calls the embedded
/// formatter artifact's raw export, and returns the initialized byte length.
fn number_format_module() -> Module {
    Module {
        name: "NumberFormat".into(),
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        types: Vec::new(),
        strings: Vec::new(),
        dependencies: Default::default(),
        layout: None,
        imports: vec![
            Import {
                runtime: None,
                symbol: crate::abi::REALLOC_SYMBOL,
                parameters: vec![ValueType::I32; 4],
                result: Some(ValueType::I32),
            },
            Import {
                runtime: None,
                symbol: crate::abi::NUMBER_TO_STRING_SYMBOL,
                parameters: vec![ValueType::F64, ValueType::I32, ValueType::I32],
                result: Some(ValueType::I32),
            },
        ],
        functions: vec![Function {
            state: None,
            id: crate::types::FunctionId(0),
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: ValueType::F64,
                },
                ValueDecl {
                    id: ValueId(2),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(3),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(4),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(5),
                    ty: ValueType::I32,
                },
            ],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::NumberConstant {
                        destination: ValueId(1),
                        value: "1e21".into(),
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: ValueId(3),
                        value: 0,
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: ValueId(4),
                        value: 1,
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: ValueId(5),
                        value: psrs_runtime::NUMBER_CAPACITY as i32,
                        span: span(),
                    },
                    Instruction::Call {
                        destination: ValueId(2),
                        function: crate::abi::REALLOC_SYMBOL,
                        arguments: vec![ValueId(3), ValueId(3), ValueId(4), ValueId(5)],
                        span: span(),
                    },
                    Instruction::Call {
                        destination: ValueId(0),
                        function: crate::abi::NUMBER_TO_STRING_SYMBOL,
                        arguments: vec![ValueId(1), ValueId(2), ValueId(5)],
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: ValueId(0),
                    span: span(),
                }),
            }],
            result: ValueId(0),
            result_type: ValueType::I32,
            span: span(),
        }],
        span: span(),
    }
}

/// Lowers and runs a MIR module through the default world's checked plan,
/// attaching the runtime artifact through component composition.
fn run_with_checked_plan(mir: &Module) -> Option<std::process::Output> {
    let target = crate::TargetCapabilities::default();
    let context = crate::linking::default_context().expect("the default world resolves");
    let mut wasi = registry();
    let link = crate::linking::plan_for_module(&context, mir, &mut wasi, target)
        .expect("the formatter module should plan");
    assert!(
        !link.plan.artifacts().is_empty(),
        "the formatter plan should select the runtime artifact"
    );
    let wasm = crate::wasm::lower_module_with_plan(mir, &mut wasi, target, &link)
        .expect("the formatter module should lower");
    let core = crate::wasm::encode_module(&wasm).expect("encoding the formatter module");
    crate::validator_for(target)
        .validate_all(&core)
        .expect("the core module should validate");
    let component = crate::linking::compose(&link, &core, mir.span, None)
        .expect("the formatter component should compose");
    crate::validator_for(target)
        .validate_all(&component)
        .expect("the component should validate");

    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: wasmtime is not installed");
        return None;
    }
    let path = std::env::temp_dir().join(format!("psrs-format-{}.wasm", std::process::id()));
    std::fs::write(&path, &component).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Some(output)
}

/// The formatter artifact is selected by the checked plan, shares the
/// application's memory above the allocator boundary, and returns the
/// initialized token length through the command exit code.
#[test]
fn formats_a_number_through_the_runtime_artifact() {
    let mir = number_format_module();
    let Some(output) = run_with_checked_plan(&mir) else {
        return;
    };
    assert_eq!(
        output.status.code(),
        Some(5),
        "1e21 formats to the 5-byte token \"1e+21\": {output:?}"
    );
}
