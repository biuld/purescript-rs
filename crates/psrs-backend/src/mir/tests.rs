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

fn registry() -> crate::abi::WasiRegistry {
    crate::abi::WasiRegistry::load().expect("the vendored WASI WIT should load")
}

/// MIR owns a defined-type table; the Wasm lowering must carry it into the
/// encoded type section ahead of the function types.
#[test]
fn defined_types_flow_into_the_wasm_type_section() {
    let mir = Module {
        name: "MirTypes".into(),
        entry: Some(SymbolId::new(ModuleId(0), 0)),
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

    let wasm = crate::wasm::lower_module(&mir, &mut registry()).expect("lowering to Wasm");
    assert_eq!(wasm.defined_type_count(), 1);
    assert_eq!(wasm.functions[0].type_index, 1);
    let binary = crate::wasm::encode_module(&wasm).expect("encoding");
    crate::validator()
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
        entry: Some(SymbolId::new(ModuleId(0), 0)),
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

    let wasm = crate::wasm::lower_module(&mir, &mut registry()).expect("lowering to Wasm");
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
    let (resolve, world) = crate::component::command_world().expect("WASI WIT should load");
    let component = crate::component::componentize(&core, &resolve, world).expect("componentizing");
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

/// A MIR module that calls a declared import: the Wasm lowering names the
/// import from the ABI registry and the module validates. Canonical argument
/// adaptation is covered through the driver's `log` test.
#[test]
fn lowers_an_imported_call() {
    let mut registry = registry();
    let import = registry
        .import(crate::abi::names::STDOUT, crate::abi::names::GET_STDOUT)
        .expect("get-stdout should resolve");
    let callee = import.symbol;
    let mir = Module {
        name: "MirImport".into(),
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        types: Vec::new(),
        imports: vec![Import {
            symbol: callee,
            parameters: import.parameters.clone(),
            result: import.result,
        }],
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
                instructions: vec![Instruction::Call {
                    destination: ValueId(0),
                    function: callee,
                    arguments: Vec::new(),
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

    let wasm = crate::wasm::lower_module(&mir, &mut registry).expect("lowering to Wasm");
    let imported = wasm
        .imports
        .iter()
        .find(|import| import.name == "get-stdout")
        .expect("the WASI import should be declared");
    assert_eq!(imported.module, "wasi:cli/stdout@0.2.12");
    let binary = crate::wasm::encode_module(&wasm).expect("encoding");
    crate::validator()
        .validate_all(&binary)
        .expect("the encoded module should validate");
}

/// The Wasm lowering selects the entry from the explicit symbol, not from a
/// declaration's source name.
#[test]
fn wasm_lowering_requires_an_explicit_entry_symbol() {
    let mir = Module {
        name: "NoEntry".into(),
        entry: None,
        types: Vec::new(),
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
                    value: 1,
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

    let errors = crate::wasm::lower_module(&mir, &mut registry()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("no program entry point")),
        "{errors:?}"
    );
}

/// The MIR verifier checks struct field types, not just the type index range.
#[test]
fn rejects_a_struct_new_with_a_mistyped_field() {
    let mir = Module {
        name: "BadStruct".into(),
        entry: None,
        types: vec![RecGroup(vec![DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Struct(vec![FieldType {
                storage: StorageType::I64,
                mutable: false,
            }]),
        }])],
        imports: Vec::new(),
        functions: vec![Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: ValueType::I64,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: ValueType::I32,
                },
                ValueDecl {
                    id: ValueId(2),
                    ty: ValueType::Ref(RefType {
                        nullable: false,
                        heap: HeapType::Index(0),
                    }),
                },
            ],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    Instruction::Constant {
                        destination: ValueId(1),
                        value: 1,
                        span: span(),
                    },
                    Instruction::StructNew {
                        destination: ValueId(2),
                        type_index: 0,
                        arguments: vec![ValueId(1)],
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: ValueId(0),
                    span: span(),
                }),
            }],
            result: ValueId(0),
            result_type: ValueType::I64,
            span: span(),
        }],
        span: span(),
    };

    let errors = crate::mir::verify_module(&mir).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("struct.new argument has the wrong type")),
        "{errors:?}"
    );
}
