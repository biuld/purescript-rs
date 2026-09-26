use super::*;
use crate::types::{DataId, MemoryId};
use crate::wasm::{
    DataIndex, DataMode, DataSegment, Export, ExportIndex, ExportKind, FuncType, FunctionIndex,
    Memory, MemoryIndex, Module,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{Instruction as I, ValType};

const HEAP_POINTER: u32 = 64;
const INITIAL_FREE: u32 = 80;
const REALLOC_INDEX: u32 = 5;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn leaf(body: &mut Body, instruction: I<'static>) {
    body.push(Op::Leaf(instruction));
}

fn allocator_call(
    body: &mut Body,
    old_ptr: I<'static>,
    old_len: I<'static>,
    align: I<'static>,
    new_len: I<'static>,
) {
    leaf(body, old_ptr);
    leaf(body, old_len);
    leaf(body, align);
    leaf(body, new_len);
    leaf(body, I::Call(REALLOC_INDEX));
}

fn function(name: &str, body: Body, locals: Vec<ValType>) -> Function {
    Function {
        symbol: SymbolId::new(ModuleId(0), 0),
        name: name.into(),
        type_index: TypeIndex(0),
        parameters: Vec::new(),
        locals,
        body,
        span: span(),
    }
}

fn fixture() -> Module {
    let mut check = Body::new();
    allocator_call(
        &mut check,
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(1),
        I::I32Const(3),
    );
    leaf(&mut check, I::LocalSet(0));
    for (offset, value) in [(0, 0x12), (1, 0x34), (2, 0x56)] {
        leaf(&mut check, I::LocalGet(0));
        if offset != 0 {
            leaf(&mut check, I::I32Const(offset));
            leaf(&mut check, I::I32Add);
        }
        leaf(&mut check, I::I32Const(value));
        leaf(&mut check, I::I32Store8(memarg(0)));
    }
    leaf(&mut check, I::LocalGet(0));
    leaf(&mut check, I::I32Const(3));
    leaf(&mut check, I::I32Const(16));
    leaf(&mut check, I::I32Const(5));
    leaf(&mut check, I::Call(REALLOC_INDEX));
    leaf(&mut check, I::LocalSet(1));
    leaf(&mut check, I::LocalGet(1));
    leaf(&mut check, I::I32Const(5));
    leaf(&mut check, I::I32Const(8));
    leaf(&mut check, I::I32Const(2));
    leaf(&mut check, I::Call(REALLOC_INDEX));
    leaf(&mut check, I::LocalSet(2));
    leaf(&mut check, I::LocalGet(2));
    leaf(&mut check, I::I32Const(7));
    leaf(&mut check, I::I32And);
    leaf(&mut check, I::I32Eqz);
    for (offset, expected) in [(0, 0x12), (1, 0x34)] {
        leaf(&mut check, I::LocalGet(2));
        if offset != 0 {
            leaf(&mut check, I::I32Const(offset));
            leaf(&mut check, I::I32Add);
        }
        leaf(&mut check, I::I32Load8U(memarg(0)));
        leaf(&mut check, I::I32Const(expected));
        leaf(&mut check, I::I32Eq);
        leaf(&mut check, I::I32And);
    }
    leaf(&mut check, I::LocalGet(2));
    leaf(&mut check, I::I32Const(4));
    leaf(&mut check, I::I32Sub);
    leaf(&mut check, I::I32Load(memarg(2)));
    leaf(&mut check, I::I32Const(2));
    leaf(&mut check, I::I32Eq);
    leaf(&mut check, I::I32And);
    allocator_call(
        &mut check,
        I::I32Const(-1),
        I::I32Const(-1),
        I::I32Const(1),
        I::I32Const(0),
    );
    leaf(&mut check, I::LocalSet(3));
    leaf(&mut check, I::LocalGet(3));
    leaf(&mut check, I::I32Eqz);
    leaf(&mut check, I::I32And);

    let mut bad_alignment = Body::new();
    allocator_call(
        &mut bad_alignment,
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(3),
        I::I32Const(1),
    );

    let mut grow_failure = Body::new();
    allocator_call(
        &mut grow_failure,
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(4),
        I::I32Const(200000),
    );

    let mut end_overflow = Body::new();
    leaf(&mut end_overflow, I::I32Const(HEAP_POINTER as i32));
    leaf(&mut end_overflow, I::I32Const(0x7fff_fffc));
    leaf(&mut end_overflow, I::I32Store(memarg(2)));
    allocator_call(
        &mut end_overflow,
        I::I32Const(0),
        I::I32Const(0),
        I::I32Const(4),
        I::I32Const(i32::MIN),
    );

    let mut old_range_out_of_bounds = Body::new();
    leaf(&mut old_range_out_of_bounds, I::I32Const(65532));
    leaf(&mut old_range_out_of_bounds, I::I32Const(16));
    leaf(&mut old_range_out_of_bounds, I::I32Store(memarg(2)));
    allocator_call(
        &mut old_range_out_of_bounds,
        I::I32Const(65536),
        I::I32Const(16),
        I::I32Const(4),
        I::I32Const(65536),
    );

    let functions = vec![
        function("check_reallocation", check, vec![ValType::I32; 4]),
        function("bad_alignment", bad_alignment, Vec::new()),
        function("grow_failure", grow_failure, Vec::new()),
        function("end_overflow", end_overflow, Vec::new()),
        function(
            "old_range_out_of_bounds",
            old_range_out_of_bounds,
            Vec::new(),
        ),
    ];
    let realloc_type = TypeIndex(1);
    let realloc = build_realloc(realloc_type, HEAP_POINTER, span());
    Module {
        name: "ReallocExecutionTest".into(),
        imports: Vec::new(),
        types: vec![
            FuncType {
                parameters: Vec::new(),
                results: vec![ValType::I32],
            },
            FuncType {
                parameters: vec![ValType::I32; 4],
                results: vec![ValType::I32],
            },
        ],
        type_defs: Vec::new(),
        functions,
        memories: vec![Memory {
            id: MemoryId(0),
            index: MemoryIndex(0),
            minimum: 1,
            maximum: Some(2),
        }],
        data: vec![DataSegment {
            id: DataId(0),
            index: DataIndex(0),
            mode: DataMode::Active {
                offset: HEAP_POINTER,
            },
            bytes: INITIAL_FREE.to_le_bytes().to_vec(),
        }],
        exports: [
            ("check_reallocation", FunctionIndex(0)),
            ("bad_alignment", FunctionIndex(1)),
            ("grow_failure", FunctionIndex(2)),
            ("end_overflow", FunctionIndex(3)),
            ("old_range_out_of_bounds", FunctionIndex(4)),
            ("cabi_realloc", FunctionIndex(REALLOC_INDEX)),
        ]
        .into_iter()
        .map(|(name, index)| Export {
            name: name.into(),
            kind: ExportKind::Function,
            index: ExportIndex::Function(index),
        })
        .chain(std::iter::once(Export {
            name: "memory".into(),
            kind: ExportKind::Memory,
            index: ExportIndex::Memory(MemoryIndex(0)),
        }))
        .collect(),
        entry: None,
        realloc: Some(realloc),
        helpers: Vec::new(),
        span: span(),
    }
}

#[test]
fn realloc_checks_alignment_overflow_and_growth_and_copies_old_bytes() {
    let module = fixture();
    super::super::super::verify::verify_module(&module).expect("thin Wasm module verifies");
    let binary = super::super::super::encode_module(&module).expect("Wasm encodes");
    crate::validator()
        .validate_all(&binary)
        .expect("allocator module validates");

    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping allocator execution: wasmtime is not installed");
        return;
    }

    let path = std::env::temp_dir().join(format!("psrs-realloc-{}.wasm", std::process::id()));
    std::fs::write(&path, binary).expect("write allocator test module");
    let run = |export: &str| {
        std::process::Command::new("wasmtime")
            .arg("run")
            .arg("--invoke")
            .arg(export)
            .arg(&path)
            .output()
            .expect("run allocator test module")
    };

    let copied = run("check_reallocation");
    assert!(copied.status.success(), "allocator trapped: {copied:?}");
    assert_eq!(String::from_utf8_lossy(&copied.stdout).trim(), "1");
    for export in [
        "bad_alignment",
        "grow_failure",
        "end_overflow",
        "old_range_out_of_bounds",
    ] {
        let trapped = run(export);
        assert!(!trapped.status.success(), "{export} unexpectedly succeeded");
    }
    let _ = std::fs::remove_file(path);
}
