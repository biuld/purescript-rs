use super::*;

fn invalid_array_index_module(index: i32) -> CcModule {
    let mut representations = RepresentationTable::default();
    let array = representations.reserve();
    representations.set(
        array,
        Representation::Array {
            element: ValueShape::Integer,
        },
    );
    let symbol = SymbolId::new(ModuleId(0), 0);
    CcModule {
        name: "LinearArrayBounds".into(),
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
                    kind: AssignmentKind::Constant(37),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::ArrayNew {
                        destination: ValueId(1),
                        representation: array,
                        elements: vec![ValueId(0)],
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::Constant(index),
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
    }
}

fn run_linear_trap(module: CcModule) {
    let target = crate::TargetCapabilities {
        bulk_memory: true,
        ..crate::TargetCapabilities::wasm_mvp()
    };
    let (mir, mut wasi) = crate::mir::lower_module_with_capabilities(module, target)
        .expect("the array access should lower before its runtime bounds check");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut wasi, target)
        .expect("the checked linear MIR should lower to Wasm");
    let binary = crate::wasm::encode_module(&wasm).expect("encoding linear MVP Wasm");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the checked linear MVP module should validate");
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok()
    {
        let path = std::env::temp_dir().join(format!(
            "psrs-linear-bounds-{}-{}.wasm",
            std::process::id(),
            super::next_artifact_id()
        ));
        std::fs::write(&path, binary).expect("writing checked linear MVP Wasm");
        let output = std::process::Command::new("wasmtime")
            .arg("run")
            .arg("--invoke")
            .arg(crate::abi::RUN_CORE_EXPORT)
            .arg(&path)
            .output()
            .expect("running checked linear MVP Wasm");
        let _ = std::fs::remove_file(path);
        assert!(
            !output.status.success(),
            "out-of-bounds access did not trap"
        );
    }
}

#[test]
fn aligns_linear_f64_array_payloads() {
    let mut representations = RepresentationTable::default();
    let array = representations.reserve();
    representations.set(
        array,
        Representation::Array {
            element: ValueShape::Number,
        },
    );
    let symbol = SymbolId::new(ModuleId(0), 0);
    let module = CcModule {
        name: "LinearNumberArray".into(),
        externals: Vec::new(),
        representations,
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                crate::cc::ValueDecl {
                    id: ValueId(0),
                    ty: ValueShape::Number,
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
                    ty: ValueShape::Number,
                },
                crate::cc::ValueDecl {
                    id: ValueId(4),
                    ty: ValueShape::Integer,
                },
                crate::cc::ValueDecl {
                    id: ValueId(5),
                    ty: ValueShape::Reference(Reference {
                        nullable: false,
                        heap: crate::cc::RefShape::Repr(array),
                    }),
                },
            ],
            assignments: vec![
                Assignment {
                    destination: ValueId(0),
                    kind: AssignmentKind::NumberConstant("6.25".into()),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(1),
                    kind: AssignmentKind::ArrayNew {
                        destination: ValueId(1),
                        representation: array,
                        elements: vec![ValueId(0)],
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(2),
                    kind: AssignmentKind::Constant(0),
                    span: span(),
                },
                Assignment {
                    destination: ValueId(5),
                    kind: AssignmentKind::ArrayNew {
                        destination: ValueId(5),
                        representation: array,
                        elements: vec![ValueId(0)],
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(3),
                    kind: AssignmentKind::ArrayGet {
                        destination: ValueId(3),
                        representation: array,
                        value: ValueId(5),
                        index: ValueId(2),
                    },
                    span: span(),
                },
                Assignment {
                    destination: ValueId(4),
                    kind: AssignmentKind::Constant(9),
                    span: span(),
                },
            ],
            result: ValueId(4),
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };
    let (mir, _) = crate::mir::lower_module_with_capabilities(
        module.clone(),
        crate::TargetCapabilities::wasm_mvp(),
    )
    .expect("the linear f64 array should lower");
    let instructions = &mir.functions[0].blocks[0].instructions;
    assert_eq!(
        instructions
            .iter()
            .filter(|instruction| matches!(
                instruction,
                Instruction::LinearAlloc {
                    bytes: 16,
                    alignment: 8,
                    ..
                }
            ))
            .count(),
        2
    );
    assert!(instructions.iter().any(|instruction| {
        matches!(
            instruction,
            Instruction::LinearStore {
                offset: 8,
                alignment: 8,
                ty: crate::types::ValueType::F64,
                ..
            }
        )
    }));
    assert!(instructions.iter().any(|instruction| {
        matches!(
            instruction,
            Instruction::LinearLoad {
                offset: 0,
                object_bytes: 8,
                alignment: 8,
                ty: crate::types::ValueType::F64,
                ..
            }
        )
    }));
    run_linear(module, "9");
}

#[test]
fn traps_on_negative_and_past_end_linear_array_indices() {
    run_linear_trap(invalid_array_index_module(-1));
    run_linear_trap(invalid_array_index_module(1));
}
