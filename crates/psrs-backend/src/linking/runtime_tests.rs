use super::*;
use crate::mir::{
    BasicBlock, BlockId, Function, Import, Instruction as I, RuntimeImport, Terminator,
};
use crate::types::{
    CompositeType, DefinedType, DefinedTypeId, FieldType, FunctionId, HeapType, RecGroup, RefType,
    StorageType, ValueDecl, ValueId, ValueType,
};

fn reference(id: u32) -> ValueType {
    ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(DefinedTypeId(id)),
    })
}

fn fixture() -> mir::Module {
    let span = TextRange::new(0, 1);
    let owner = ModuleId(7);
    let symbol = |id| SymbolId::new(owner, id);
    let value = |id| ValueId(id);
    let eq = ValueType::Ref(RefType {
        nullable: true,
        heap: HeapType::Eq,
    });
    let import = |id, function: &str, parameters, result| Import {
        symbol: symbol(id),
        runtime: Some(RuntimeImport {
            module: psrs_runtime::STORAGE_MODULE.into(),
            function: function.into(),
        }),
        parameters,
        result,
    };
    let types = [
        CompositeType::Struct(vec![FieldType {
            mutable: false,
            storage: StorageType::I64,
        }]),
        CompositeType::Array(FieldType {
            mutable: true,
            storage: StorageType::Ref(RefType {
                nullable: true,
                heap: HeapType::Eq,
            }),
        }),
        CompositeType::Struct(vec![FieldType {
            mutable: false,
            storage: StorageType::I32,
        }]),
    ]
    .into_iter()
    .map(|composite| {
        RecGroup(vec![DefinedType {
            final_type: true,
            supertype: None,
            composite,
        }])
    })
    .collect();
    let shapes = [
        ValueType::I32,
        ValueType::I32,
        reference(2),
        reference(1),
        ValueType::I32,
        ValueType::I32,
        reference(2),
        reference(1),
        eq,
        reference(2),
        ValueType::I32,
        eq,
        eq,
    ];
    let instructions = vec![
        I::Constant {
            destination: value(0),
            value: 1,
            span,
        },
        I::Constant {
            destination: value(1),
            value: 40,
            span,
        },
        I::StructNew {
            destination: value(2),
            type_index: DefinedTypeId(2),
            arguments: vec![value(1)],
            span,
        },
        I::RefCast {
            destination: value(11),
            value: value(2),
            reference: RefType {
                nullable: true,
                heap: HeapType::Eq,
            },
            span,
        },
        I::Call {
            destination: value(3),
            function: symbol(11),
            arguments: vec![value(0), value(11)],
            span,
        },
        I::Constant {
            destination: value(4),
            value: 0,
            span,
        },
        I::Constant {
            destination: value(5),
            value: 99,
            span,
        },
        I::StructNew {
            destination: value(6),
            type_index: DefinedTypeId(2),
            arguments: vec![value(5)],
            span,
        },
        I::Copy {
            destination: value(7),
            value: value(3),
            span,
        },
        I::RefCast {
            destination: value(12),
            value: value(6),
            reference: RefType {
                nullable: true,
                heap: HeapType::Eq,
            },
            span,
        },
        I::CallVoid {
            function: symbol(12),
            arguments: vec![value(3), value(4), value(12)],
            span,
        },
        I::Call {
            destination: value(8),
            function: symbol(13),
            arguments: vec![value(7), value(4)],
            span,
        },
        I::RefCast {
            destination: value(9),
            value: value(8),
            reference: RefType {
                nullable: false,
                heap: HeapType::Index(DefinedTypeId(2)),
            },
            span,
        },
        I::StructGet {
            destination: value(10),
            type_index: DefinedTypeId(2),
            field: 0,
            value: value(9),
            span,
        },
    ];
    mir::Module {
        name: "RuntimeStorageConsumer".into(),
        types,
        strings: vec![],
        imports: vec![
            import(
                11,
                "array_fill",
                vec![ValueType::I32, eq],
                Some(reference(1)),
            ),
            import(
                12,
                "array_write",
                vec![reference(1), ValueType::I32, eq],
                None,
            ),
            import(
                13,
                "array_read",
                vec![reference(1), ValueType::I32],
                Some(eq),
            ),
        ],
        functions: vec![Function {
            state: None,
            id: FunctionId(0),
            symbol: symbol(20),
            name: "main".into(),
            parameters: vec![],
            values: shapes
                .into_iter()
                .enumerate()
                .map(|(id, ty)| ValueDecl {
                    id: value(id as u32),
                    ty,
                })
                .collect(),
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: vec![],
                instructions,
                terminator: Some(Terminator::Return {
                    value: value(10),
                    span,
                }),
            }],
            result: value(10),
            result_type: ValueType::I32,
            span,
        }],
        entry: Some(symbol(20)),
        dependencies: Default::default(),
        layout: None,
        span,
    }
}

#[test]
fn runtime_import_contracts_are_checked_by_binding_and_linking_owners() {
    let module = fixture();
    mir::verify_module(&module).unwrap();
    let context = default_context().unwrap();
    let target = TargetCapabilities::default();
    let mut wasi = WasiRegistry::load().unwrap();
    let plan = plan_for_module(&context, &module, &mut wasi, target).unwrap();
    assert_eq!(plan.plan.artifacts().len(), 1);
    assert_eq!(
        plan.imports[&module.imports[0].symbol],
        (psrs_runtime::STORAGE_MODULE.into(), "array_fill".into())
    );
    for change in 0..6 {
        let mut wrong = module.clone();
        let import = &mut wrong.imports[0];
        match change {
            0 => import.runtime.as_mut().unwrap().module = "wrong-provider".into(),
            1 => import.runtime.as_mut().unwrap().function = "array_read".into(),
            2 => import.parameters[0] = ValueType::I64,
            3 => import.result = Some(ValueType::I32),
            4 => {
                import.result = Some(ValueType::Ref(RefType {
                    nullable: true,
                    heap: HeapType::Index(DefinedTypeId(1)),
                }))
            }
            _ => import.symbol = crate::abi::REALLOC_SYMBOL,
        }
        // Provider identity is opaque to MIR. Ordinary call typing remains
        // checked there, while provider selection belongs to the link boundary.
        if change == 0 || change == 1 {
            mir::verify_module(&wrong).unwrap();
        }
        assert!(
            crate::bindings::verify_runtime_import(&wrong.imports[0], &wrong.types).is_err(),
            "change {change}"
        );
        assert!(
            plan_for_module(&context, &wrong, &mut wasi, target).is_err(),
            "change {change}"
        );
    }
    let mut wrong = module.clone();
    wrong.types[1].0[0].final_type = false;
    mir::verify_module(&wrong).unwrap();
    assert!(
        plan_for_module(&context, &wrong, &mut wasi, target).is_err(),
        "linking must compare the actual GC definition with the runtime catalog"
    );
}

#[test]
fn backend_runtime_storage_imports_compose_and_execute_an_alias_write() {
    let mut module = fixture();
    let function = &mut module.functions[0];
    function.values.extend([
        ValueDecl {
            id: ValueId(13),
            ty: ValueType::I32,
        },
        ValueDecl {
            id: ValueId(14),
            ty: ValueType::Boolean,
        },
        ValueDecl {
            id: ValueId(15),
            ty: ValueType::I32,
        },
    ]);
    function.blocks[0].instructions.extend([
        I::Constant {
            destination: ValueId(13),
            value: 99,
            span: module.span,
        },
        I::Primitive {
            destination: ValueId(14),
            op: mir::NumericOp::I32Ne,
            left: ValueId(10),
            right: ValueId(13),
            span: module.span,
        },
        I::UnaryPrimitive {
            destination: ValueId(15),
            op: mir::UnaryOp::BoolToI32,
            value: ValueId(14),
            span: module.span,
        },
    ]);
    function.result = ValueId(15);
    function.blocks[0].terminator = Some(Terminator::Return {
        value: ValueId(15),
        span: module.span,
    });
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
    let target = TargetCapabilities::default();
    let context = default_context().unwrap();
    for expected in [99, 100] {
        let instruction = module.functions[0].blocks[0]
            .instructions
            .iter_mut()
            .find(|instruction| {
                matches!(
                    instruction,
                    I::Constant {
                        destination: ValueId(13),
                        ..
                    }
                )
            })
            .unwrap();
        let I::Constant { value, .. } = instruction else {
            unreachable!()
        };
        *value = expected;
        mir::verify_module(&module).unwrap();
        let mut wasi = WasiRegistry::load().unwrap();
        let plan = plan_for_module(&context, &module, &mut wasi, target).unwrap();
        let wasm = crate::wasm::lower_module_with_plan(&module, &mut wasi, target, &plan).unwrap();
        let core = crate::wasm::encode_module(&wasm).unwrap();
        let component = compose(
            &plan,
            &core,
            module.span,
            module.entry.map(|entry| entry.module),
        )
        .unwrap();
        crate::validator_for(target)
            .validate_all(&component)
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "psrs-backend-storage-{}-{expected}.wasm",
            std::process::id()
        ));
        std::fs::write(&path, component).unwrap();
        let output = std::process::Command::new("wasmtime")
            .arg("run")
            .arg(&path)
            .output()
            .unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            output.status.code(),
            Some(i32::from(expected != 99)),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
