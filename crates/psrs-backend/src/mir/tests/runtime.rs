//! End-to-end execution of a lowered MIR module under a Wasm runtime.
use super::{registry, span};
use crate::mir::{BasicBlock, BlockId, Function, Instruction, Module, NumericOp, Terminator};
use crate::types::{
    CompositeType, DefinedType, FieldType, HeapType, RecGroup, RefType, StorageType, ValueDecl,
    ValueId, ValueType,
};
use psrs_hir::{ModuleId, SymbolId};

/// A MIR function that builds a GC struct and sums its fields, lowered through
/// the Wasm backend and executed under wasmtime.
#[test]
fn runs_a_mir_gc_struct_under_wasmtime() {
    let struct_type = DefinedType {
        final_type: true,
        supertype: None,
        composite: CompositeType::Struct(vec![
            FieldType {
                storage: StorageType::I32,
                mutable: false,
            },
            FieldType {
                storage: StorageType::I32,
                mutable: false,
            },
        ]),
    };
    let reference = ValueType::Ref(RefType {
        nullable: false,
        heap: HeapType::Index(crate::types::DefinedTypeId(0)),
    });
    let mir = Module {
        name: "MirGc".into(),
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        types: vec![RecGroup(vec![struct_type])],
        strings: Vec::new(),
        dependencies: Default::default(),
        layout: None,
        imports: Vec::new(),
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
                    ty: reference,
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
                    Instruction::Constant {
                        destination: ValueId(2),
                        value: 3,
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: ValueId(3),
                        value: 4,
                        span: span(),
                    },
                    Instruction::StructNew {
                        destination: ValueId(1),
                        type_index: crate::types::DefinedTypeId(0),
                        arguments: vec![ValueId(2), ValueId(3)],
                        span: span(),
                    },
                    Instruction::StructGet {
                        destination: ValueId(4),
                        type_index: crate::types::DefinedTypeId(0),
                        field: 0,
                        value: ValueId(1),
                        span: span(),
                    },
                    Instruction::StructGet {
                        destination: ValueId(5),
                        type_index: crate::types::DefinedTypeId(0),
                        field: 1,
                        value: ValueId(1),
                        span: span(),
                    },
                    Instruction::Primitive {
                        destination: ValueId(0),
                        op: NumericOp::I32Add,
                        left: ValueId(4),
                        right: ValueId(5),
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
    };

    let context = crate::linking::default_context().expect("resolved world");
    let mut wasi = registry();
    let target = crate::TargetCapabilities::default();
    let link = crate::linking::plan_for_module(&context, &mir, &mut wasi, target)
        .expect("checked target plan");
    let wasm = crate::wasm::lower_module_with_plan(&mir, &mut wasi, target, &link)
        .expect("lowering to Wasm");
    let core = crate::wasm::encode_module(&wasm).expect("encoding");
    crate::validator()
        .validate_all(&core)
        .expect("the encoded module should validate");

    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: wasmtime is not installed");
        return;
    }
    let component = crate::linking::compose(&link, &core, mir.span, None).expect("componentizing");
    let path = std::env::temp_dir().join(format!("psrs-mir-gc-{}.wasm", std::process::id()));
    std::fs::write(&path, &component).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(7), "wasmtime output: {output:?}");
}
