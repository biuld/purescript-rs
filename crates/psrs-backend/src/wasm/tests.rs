use super::*;
use crate::types::{CompositeType, DefinedType, FieldType, RecGroup, StorageType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{
    BlockType, HeapType, Instruction, RefType as WasmRefType, ValType as WasmValType,
};

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn module_with_function_body(name: &str, body: Body) -> Module {
    Module {
        name: name.into(),
        imports: Vec::new(),
        types: vec![FuncType {
            parameters: Vec::new(),
            results: Vec::new(),
        }],
        type_defs: Vec::new(),
        functions: vec![Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: name.into(),
            type_index: super::TypeIndex(0),
            parameters: Vec::new(),
            locals: Vec::new(),
            body,
            span: span(),
        }],
        memories: Vec::new(),
        data: Vec::new(),
        exports: Vec::new(),
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: Vec::new(),
        span: span(),
    }
}

fn struct_ref(index: u32) -> WasmValType {
    WasmValType::Ref(WasmRefType {
        nullable: false,
        heap_type: HeapType::Concrete(index),
    })
}

/// A language-agnostic check that the thin Wasm IR and encoder can declare and
/// use a defined GC struct type.
#[test]
fn encodes_and_runs_a_gc_struct() {
    let group = RecGroup(vec![DefinedType {
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
    }]);
    let module = Module {
        name: "GcTest".into(),
        imports: Vec::new(),
        types: vec![FuncType {
            parameters: Vec::new(),
            results: vec![WasmValType::I32],
        }],
        type_defs: vec![group],
        functions: vec![Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "sum".into(),
            type_index: super::TypeIndex(1),
            parameters: Vec::new(),
            locals: vec![struct_ref(0)],
            body: vec![
                Op::Leaf(Instruction::I32Const(3)),
                Op::Leaf(Instruction::I32Const(4)),
                Op::Leaf(Instruction::StructNew(0)),
                Op::Leaf(Instruction::LocalSet(0)),
                Op::Leaf(Instruction::LocalGet(0)),
                Op::Leaf(Instruction::StructGet {
                    struct_type_index: 0,
                    field_index: 0,
                }),
                Op::Leaf(Instruction::LocalGet(0)),
                Op::Leaf(Instruction::StructGet {
                    struct_type_index: 0,
                    field_index: 1,
                }),
                Op::Leaf(Instruction::I32Add),
            ],
            span: span(),
        }],
        memories: Vec::new(),
        data: Vec::new(),
        exports: vec![Export {
            name: "sum".into(),
            kind: ExportKind::Function,
            index: super::ExportIndex::Function(super::FunctionIndex(0)),
        }],
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: Vec::new(),
        span: span(),
    };

    super::verify::verify_module(&module).unwrap();
    let binary = super::encode_module(&module).unwrap();
    crate::validator()
        .validate_all(&binary)
        .expect("the encoded GC module should validate");

    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: wasmtime is not installed");
        return;
    }
    let path = std::env::temp_dir().join(format!("psrs-gc-{}.wasm", std::process::id()));
    std::fs::write(&path, &binary).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg("--invoke")
        .arg("sum")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(output.status.success(), "wasmtime failed: {output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "7");
}

#[test]
fn defined_types_precede_function_types() {
    let module = Module {
        name: "Empty".into(),
        imports: Vec::new(),
        types: vec![FuncType {
            parameters: Vec::new(),
            results: Vec::new(),
        }],
        type_defs: vec![RecGroup(vec![DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Struct(Vec::new()),
        }])],
        functions: Vec::new(),
        memories: Vec::new(),
        data: Vec::new(),
        exports: Vec::new(),
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: Vec::new(),
        span: span(),
    };
    assert_eq!(module.defined_type_count(), 1);
}

#[test]
fn rejects_a_data_index_that_does_not_match_module_order() {
    let module = Module {
        name: "DataIndex".into(),
        imports: Vec::new(),
        types: Vec::new(),
        type_defs: Vec::new(),
        functions: Vec::new(),
        memories: Vec::new(),
        data: vec![DataSegment {
            id: crate::types::DataId(0),
            index: super::DataIndex(1),
            mode: super::DataMode::Active { offset: 0 },
            bytes: Vec::new(),
        }],
        exports: Vec::new(),
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: Vec::new(),
        span: span(),
    };
    let errors = super::verify::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("data segment")),
        "{errors:?}"
    );
}

#[test]
fn rejects_an_export_with_the_wrong_index_domain() {
    let module = Module {
        name: "ExportIndex".into(),
        imports: Vec::new(),
        types: Vec::new(),
        type_defs: Vec::new(),
        functions: Vec::new(),
        memories: Vec::new(),
        data: Vec::new(),
        exports: vec![Export {
            name: "bad".into(),
            kind: ExportKind::Function,
            index: super::ExportIndex::Memory(super::MemoryIndex(0)),
        }],
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: Vec::new(),
        span: span(),
    };
    let errors = super::verify::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("unknown index")),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_branch_depth_outside_its_enclosing_labels() {
    let module = module_with_function_body("BadBranchDepth", vec![Op::Leaf(Instruction::Br(1))]);
    let errors = super::verify::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("does not target an enclosing label")),
        "{errors:?}"
    );
}

#[test]
fn accepts_a_branch_to_the_function_label_from_a_nested_block() {
    let module = module_with_function_body(
        "BranchToFunctionLabel",
        vec![Op::Block {
            body: vec![
                Op::Leaf(Instruction::I32Const(1)),
                Op::Leaf(Instruction::BrIf(1)),
                Op::Leaf(Instruction::Br(1)),
            ],
            result: None,
            span: span(),
        }],
    );

    super::verify::verify_module(&module).expect("the function label is an active target");
    let binary = super::encode_module(&module).expect("the structured body should encode");
    crate::validator()
        .validate_all(&binary)
        .expect("the branch should target the implicit function label");
}

#[test]
fn accepts_branches_to_the_function_label_through_raw_wasm_labels() {
    let module = module_with_function_body(
        "RawLabelsToFunctionLabel",
        vec![
            Op::Leaf(Instruction::Block(BlockType::Empty)),
            Op::Leaf(Instruction::Loop(BlockType::Empty)),
            Op::Leaf(Instruction::I32Const(1)),
            Op::Leaf(Instruction::BrIf(2)),
            Op::Leaf(Instruction::Br(2)),
            Op::Leaf(Instruction::End),
            Op::Leaf(Instruction::End),
        ],
    );

    super::verify::verify_module(&module).expect("raw labels contribute to branch depth");
    let binary = super::encode_module(&module).expect("the raw control flow should encode");
    crate::validator()
        .validate_all(&binary)
        .expect("the raw branch targets should pass Wasm validation");
}

#[test]
fn rejects_a_branch_depth_beyond_the_structured_labels() {
    let module = module_with_function_body(
        "BadStructuredBranchDepth",
        vec![Op::Block {
            body: vec![Op::Leaf(Instruction::Br(2))],
            result: None,
            span: span(),
        }],
    );

    let errors = super::verify::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("does not target an enclosing label")),
        "{errors:?}"
    );
}

/// A literal used twice is materialized once: MIR interning yields one passive
/// data segment and one lazily initialized global, each use guarded by
/// `ref.is_null`/`global.set`, and the encoded module validates.
#[test]
fn interns_repeated_string_literals_in_one_lazy_global() {
    use crate::mir::{
        BasicBlock, BlockId, Function as MirFunction, Instruction as MirInstruction,
        Module as MirModule, Terminator,
    };
    use crate::types::{
        DataId, DefinedTypeId, FunctionId, HeapType as MirHeapType, RefType as MirRefType,
        ValueDecl, ValueId, ValueType,
    };

    let string = ValueType::Ref(MirRefType {
        nullable: false,
        heap: MirHeapType::Index(DefinedTypeId(0)),
    });
    let symbol = SymbolId::new(ModuleId(0), 0);
    let mir = MirModule {
        name: "InternedStrings".into(),
        entry: Some(symbol),
        types: vec![RecGroup(vec![DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Array(FieldType {
                storage: StorageType::I16,
                mutable: true,
            }),
        }])],
        strings: vec!["twice".into()],
        imports: Vec::new(),
        functions: vec![MirFunction {
            id: FunctionId(0),
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: vec![
                ValueDecl {
                    id: ValueId(0),
                    ty: string,
                },
                ValueDecl {
                    id: ValueId(1),
                    ty: string,
                },
                ValueDecl {
                    id: ValueId(2),
                    ty: ValueType::I32,
                },
            ],
            entry: BlockId(0),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                parameters: Vec::new(),
                instructions: vec![
                    MirInstruction::ArrayNewData {
                        destination: ValueId(0),
                        type_index: DefinedTypeId(0),
                        data_index: DataId(0),
                        span: span(),
                    },
                    MirInstruction::ArrayNewData {
                        destination: ValueId(1),
                        type_index: DefinedTypeId(0),
                        data_index: DataId(0),
                        span: span(),
                    },
                    MirInstruction::Constant {
                        destination: ValueId(2),
                        value: 7,
                        span: span(),
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: ValueId(2),
                    span: span(),
                }),
            }],
            result: ValueId(2),
            result_type: ValueType::I32,
            span: span(),
        }],
        span: span(),
    };

    let mut registry = crate::abi::WasiRegistry::load().expect("the vendored WASI WIT should load");
    let wasm = crate::wasm::lower_module(&mir, &mut registry).expect("lowering a repeated literal");
    assert_eq!(wasm.data.len(), 1, "one deduplicated passive data segment");
    assert_eq!(
        wasm.globals.len(),
        1,
        "one interned global per distinct literal"
    );
    assert!(wasm.globals[0].mutable);
    assert_eq!(
        wasm.globals[0].ty,
        WasmValType::Ref(WasmRefType {
            nullable: true,
            heap_type: HeapType::Concrete(0),
        })
    );
    assert_eq!(
        wasm.globals[0].init,
        super::GlobalInit::RefNull(HeapType::Concrete(0))
    );

    let binary = crate::wasm::encode_module(&wasm).expect("encoding the interned module");
    crate::validator()
        .validate_all(&binary)
        .expect("the encoded interned module should validate");
    let wat = wasmprinter::print_bytes(&binary).expect("printing the core module");
    assert_eq!(wat.matches("array.new_data").count(), 2);
    assert_eq!(wat.matches("ref.is_null").count(), 2);
    assert_eq!(wat.matches("global.set 0").count(), 2);
    assert_eq!(wat.matches("global.get 0").count(), 4);
    assert!(wat.contains("(mut (ref null 0))"));
}

#[test]
fn rejects_a_branch_depth_beyond_the_raw_labels() {
    let module = module_with_function_body(
        "BadRawBranchDepth",
        vec![
            Op::Leaf(Instruction::Block(BlockType::Empty)),
            Op::Leaf(Instruction::Loop(BlockType::Empty)),
            Op::Leaf(Instruction::Br(3)),
            Op::Leaf(Instruction::End),
            Op::Leaf(Instruction::End),
        ],
    );

    let errors = super::verify::verify_module(&module).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("does not target an enclosing label")),
        "{errors:?}"
    );
}
