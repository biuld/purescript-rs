use super::*;
use crate::types::{CompositeType, DefinedType, FieldType, RecGroup, StorageType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{HeapType, Instruction, RefType as WasmRefType, ValType as WasmValType};

fn span() -> TextRange {
    TextRange::new(0, 1)
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
            type_index: 1,
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
            index: 0,
        }],
        entry: None,
        realloc: None,
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
        span: span(),
    };
    assert_eq!(module.defined_type_count(), 1);
}
