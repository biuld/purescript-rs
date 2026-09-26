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
