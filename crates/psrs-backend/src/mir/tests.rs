use super::{BasicBlock, BlockId, Function, Import, Instruction, Module, Terminator};
use crate::types::{
    CompositeType, DefinedType, FieldType, HeapType, RecGroup, RefType, StorageType, ValueDecl,
    ValueId, ValueType,
};
use psrs_core::Primitive;
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

/// MIR owns a defined-type table; the Wasm lowering must carry it into the
/// encoded type section ahead of the function types.
#[test]
fn defined_types_flow_into_the_wasm_type_section() {
    let mir = Module {
        name: "MirTypes".into(),
        externals: Vec::new(),
        types: vec![RecGroup(vec![DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Struct(vec![FieldType {
                storage: StorageType::I32,
                mutable: false,
            }]),
        }])],
        imports: Vec::new(),
        functions: vec![Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![ValueDecl {
                id: ValueId(0),
                ty: ValueType::I32,
            }],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![Instruction::Constant {
                    destination: ValueId(0),
                    value: 42,
                    span: span(),
                }],
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

    let wasm = crate::wasm::lower_module(&mir).expect("lowering to Wasm");
    assert_eq!(wasm.defined_type_count(), 1);
    assert_eq!(wasm.functions[0].type_index, 1);
    let binary = crate::wasm::encode_module(&wasm).expect("encoding");
    wasmparser::Validator::new()
        .validate_all(&binary)
        .expect("the encoded module should validate");
}

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
        heap: HeapType::Index(0),
    });
    let mir = Module {
        name: "MirGc".into(),
        externals: Vec::new(),
        types: vec![RecGroup(vec![struct_type])],
        imports: Vec::new(),
        functions: vec![Function {
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
                        type_index: 0,
                        arguments: vec![ValueId(2), ValueId(3)],
                        span: span(),
                    },
                    Instruction::StructGet {
                        destination: ValueId(4),
                        type_index: 0,
                        field: 0,
                        value: ValueId(1),
                        span: span(),
                    },
                    Instruction::StructGet {
                        destination: ValueId(5),
                        type_index: 0,
                        field: 1,
                        value: ValueId(1),
                        span: span(),
                    },
                    Instruction::Primitive {
                        destination: ValueId(0),
                        op: Primitive::Add,
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

    let wasm = crate::wasm::lower_module(&mir).expect("lowering to Wasm");
    let binary = crate::wasm::encode_module(&wasm).expect("encoding");
    wasmparser::Validator::new()
        .validate_all(&binary)
        .expect("the encoded module should validate");

    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: wasmtime is not installed");
        return;
    }
    let path = std::env::temp_dir().join(format!("psrs-mir-gc-{}.wasm", std::process::id()));
    std::fs::write(&path, &binary).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(7), "wasmtime output: {output:?}");
}

/// A MIR module that calls the WIT-derived runtime import through the canonical
/// ABI: read a string's length from its length-prefixed buffer, pass the data
/// pointer and length, and call the void `log` import.
#[test]
fn lowers_a_runtime_abi_import_call() {
    let log = psrs_hir::RuntimeFunction::ConsoleLog.symbol();
    let mir = Module {
        name: "MirImport".into(),
        externals: Vec::new(),
        types: Vec::new(),
        imports: vec![Import {
            symbol: log,
            module: "psrs:runtime/runtime".into(),
            name: "log".into(),
            parameters: vec![ValueType::I32, ValueType::I32],
            result: None,
        }],
        functions: vec![Function {
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
                    ty: ValueType::I32,
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
            ],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::StringConstant {
                        destination: ValueId(1),
                        bytes: "hi".into(),
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: ValueId(4),
                        value: 4,
                        span: span(),
                    },
                    Instruction::Load {
                        destination: ValueId(2),
                        address: ValueId(1),
                        offset: 0,
                        span: span(),
                    },
                    Instruction::Primitive {
                        destination: ValueId(3),
                        op: Primitive::Add,
                        left: ValueId(1),
                        right: ValueId(4),
                        span: span(),
                    },
                    Instruction::CallVoid {
                        function: log,
                        arguments: vec![ValueId(3), ValueId(2)],
                        span: span(),
                    },
                    Instruction::Constant {
                        destination: ValueId(0),
                        value: 0,
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

    let wasm = crate::wasm::lower_module(&mir).expect("lowering to Wasm");
    let log_import = wasm
        .imports
        .iter()
        .find(|import| import.name == "log")
        .expect("the runtime import should be declared");
    assert_eq!(log_import.module, "psrs:runtime/runtime");
    let binary = crate::wasm::encode_module(&wasm).expect("encoding");
    wasmparser::Validator::new()
        .validate_all(&binary)
        .expect("the encoded module should validate");
}
