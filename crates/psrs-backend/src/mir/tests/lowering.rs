//! Wasm lowering from a checked MIR module.
use super::{registry, span};
use crate::mir::{BasicBlock, BlockId, Function, Import, Instruction, Module, Terminator, UnaryOp};
use crate::types::{
    CompositeType, DefinedType, FieldType, RecGroup, StorageType, ValueDecl, ValueId, ValueType,
};
use psrs_hir::{ModuleId, SymbolId};

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
    assert_eq!(wasm.functions[0].type_index, crate::wasm::TypeIndex(1));
    let binary = crate::wasm::encode_module(&wasm).expect("encoding");
    crate::validator()
        .validate_all(&binary)
        .expect("the encoded module should validate");
}

#[test]
fn lowers_and_validates_f64_to_f32_abi_conversions() {
    let symbol = SymbolId::new(ModuleId(0), 0);
    let mir = Module {
        name: "FloatAbi".into(),
        entry: Some(symbol),
        types: Vec::new(),
        strings: Vec::new(),
        dependencies: Default::default(),
        layout: None,
        imports: Vec::new(),
        functions: vec![Function {
            state: None,
            id: crate::types::FunctionId(0),
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: ValueType::F64,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: ValueType::F32,
                },
                ValueDecl {
                    id: ValueId(2),
                    ty: ValueType::F64,
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
                    Instruction::NumberConstant {
                        destination: ValueId(0),
                        value: "1.25".into(),
                        span: span(),
                    },
                    Instruction::UnaryPrimitive {
                        destination: ValueId(1),
                        op: UnaryOp::F64ToF32,
                        value: ValueId(0),
                        span: span(),
                    },
                    Instruction::UnaryPrimitive {
                        destination: ValueId(2),
                        op: UnaryOp::F32ToF64,
                        value: ValueId(1),
                        span: span(),
                    },
                    Instruction::UnaryPrimitive {
                        destination: ValueId(3),
                        op: UnaryOp::F64ToI32Sat,
                        value: ValueId(2),
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: ValueId(3),
                    span: span(),
                }),
            }],
            result: ValueId(3),
            result_type: ValueType::I32,
            span: span(),
        }],
        span: span(),
    };

    let wasm = crate::wasm::lower_module(&mir, &mut registry())
        .expect("f64/f32 ABI conversions should lower to Wasm");
    let binary = crate::wasm::encode_module(&wasm).expect("encoding the Wasm module");
    crate::validator()
        .validate_all(&binary)
        .expect("the encoded f64/f32 conversion sequence should validate");
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
        strings: Vec::new(),
        dependencies: Default::default(),
        layout: None,
        imports: vec![Import {
            runtime: None,
            symbol: callee,
            parameters: import.parameters.clone(),
            result: import.result,
        }],
        functions: vec![Function {
            state: None,
            id: crate::types::FunctionId(0),
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
