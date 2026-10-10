//! Cross-module ABI evidence using the actual planner and Wasm encoder.
use super::*;
use crate::cc::{Reference, Signature};
use crate::wasm::{self, Export, ExportIndex, ExportKind, FuncType, FunctionIndex, Op, TypeIndex};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{HeapType as H, Instruction as I, RefType as R, ValType as V};

fn consumer(merge_groups: bool) -> Vec<u8> {
    let mut table = RepresentationTable::default();
    let array = table.reserve();
    table.set(
        array,
        Representation::Array {
            element: CcValueShape::Reference(Reference {
                nullable: true,
                heap: CcRefShape::Erased,
            }),
        },
    );
    let boxed = table.reserve();
    table.set(
        boxed,
        Representation::Box {
            value: CcValueShape::Integer,
        },
    );
    table.add_signature(Signature {
        parameters: vec![CcValueShape::Integer],
        result: CcValueShape::Integer,
    });
    // Unrelated closure and box types must not become members of the array's
    // canonical recursion group merely because this application uses them.
    let layout = PlannedLayout::plan(&table, TargetCapabilities::default()).unwrap();
    let array = layout.repr_index(array).unwrap().0;
    let boxed = layout.repr_index(boxed).unwrap().0;
    let array_ref = |nullable| {
        V::Ref(R {
            nullable,
            heap_type: H::Concrete(array),
        })
    };
    let box_ref = V::Ref(R {
        nullable: true,
        heap_type: H::Concrete(boxed),
    });
    let type_count = layout.types.iter().map(|g| g.0.len() as u32).sum::<u32>();
    let span = TextRange::new(0, 1);
    let raw = |instruction| Op::Leaf(instruction);
    let body = vec![
        raw(I::I32Const(7)),
        raw(I::StructNew(boxed)),
        raw(I::LocalSet(2)),
        raw(I::I32Const(78)),
        raw(I::StructNew(boxed)),
        raw(I::LocalSet(3)),
        raw(I::I32Const(2)),
        raw(I::LocalGet(2)),
        raw(I::Call(0)),
        raw(I::LocalSet(0)),
        raw(I::LocalGet(0)),
        raw(I::LocalSet(1)),
        raw(I::LocalGet(0)),
        raw(I::RefAsNonNull),
        raw(I::I32Const(0)),
        raw(I::Call(1)),
        raw(I::LocalGet(2)),
        raw(I::RefEq),
        raw(I::I32Eqz),
        Op::If {
            then_body: vec![raw(I::Unreachable)],
            else_body: Vec::new(),
            result: None,
            span,
        },
        raw(I::LocalGet(0)),
        raw(I::RefAsNonNull),
        raw(I::I32Const(1)),
        raw(I::LocalGet(3)),
        raw(I::Call(2)),
        raw(I::LocalGet(1)),
        raw(I::RefAsNonNull),
        raw(I::I32Const(1)),
        raw(I::Call(1)),
        raw(I::LocalGet(3)),
        raw(I::RefEq),
        raw(I::I32Eqz),
        Op::If {
            then_body: vec![raw(I::Unreachable)],
            else_body: Vec::new(),
            result: None,
            span,
        },
        raw(I::LocalGet(1)),
        raw(I::RefAsNonNull),
        raw(I::I32Const(1)),
        raw(I::Call(1)),
        raw(I::RefCastNonNull(H::Concrete(boxed))),
        raw(I::StructGet {
            struct_type_index: boxed,
            field_index: 0,
        }),
    ];
    let imports = ["array_fill", "array_read", "array_write"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| wasm::Import {
            module: psrs_runtime::STORAGE_MODULE.into(),
            name: name.into(),
            type_index: TypeIndex(type_count + index as u32),
        })
        .collect();
    let types = vec![
        FuncType {
            parameters: vec![V::I32, V::Ref(R::EQREF)],
            results: vec![array_ref(false)],
        },
        FuncType {
            parameters: vec![array_ref(false), V::I32],
            results: vec![V::Ref(R::EQREF)],
        },
        FuncType {
            parameters: vec![array_ref(false), V::I32, V::Ref(R::EQREF)],
            results: Vec::new(),
        },
        FuncType {
            parameters: Vec::new(),
            results: vec![V::I32],
        },
    ];
    let type_defs = if merge_groups {
        vec![RecGroup(
            layout.types.into_iter().flat_map(|group| group.0).collect(),
        )]
    } else {
        layout.types
    };
    let module = wasm::Module {
        name: "storage_planned_consumer".into(),
        imports,
        types,
        type_defs,
        functions: vec![wasm::Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "test".into(),
            type_index: TypeIndex(type_count + 3),
            parameters: Vec::new(),
            locals: vec![array_ref(true), array_ref(true), box_ref, box_ref],
            body,
            span,
        }],
        memories: Vec::new(),
        data: Vec::new(),
        globals: Vec::new(),
        helpers: Vec::new(),
        entry: None,
        realloc: None,
        exports: vec![Export {
            name: "test".into(),
            kind: ExportKind::Function,
            index: ExportIndex::Function(FunctionIndex(3)),
        }],
        span,
    };
    let bytes = wasm::encode_module(&module).unwrap();
    crate::validator().validate_all(&bytes).unwrap();
    bytes
}

#[test]
fn planned_gc_array_matches_runtime_storage_and_preserves_aliases() {
    use std::process::Command;
    if !Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|version| version.status.success())
    {
        assert_ne!(
            std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(),
            Ok("1"),
            "Wasmtime is required"
        );
        return;
    }
    let directory =
        std::env::temp_dir().join(format!("psrs-planned-storage-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let provider = directory.join("storage.wasm");
    let guest = directory.join("guest.wasm");
    std::fs::write(&provider, psrs_runtime::storage::encode_module()).unwrap();
    let run = || {
        Command::new("wasmtime")
            .args(["run", "--preload"])
            .arg(format!(
                "{}={}",
                psrs_runtime::STORAGE_MODULE,
                provider.display()
            ))
            .args(["--invoke", "test"])
            .arg(&guest)
            .output()
            .unwrap()
    };
    std::fs::write(&guest, consumer(true)).unwrap();
    let rejected = run();
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("incompatible import type"),
        "{rejected:?}"
    );
    std::fs::write(&guest, consumer(false)).unwrap();
    let output = run();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "78");
}
