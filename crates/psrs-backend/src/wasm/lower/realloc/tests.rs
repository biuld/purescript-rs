use super::*;
use crate::types::{DataId, MemoryId};
use crate::wasm::lower::asm::{Asm, constant, get, set};
use crate::wasm::{
    Body, DataIndex, DataMode, DataSegment, Export, ExportIndex, ExportKind, FuncType, Function,
    FunctionIndex, Memory, MemoryIndex, Module,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{Instruction as I, MemArg, ValType};

const CHECK_COUNT: u32 = 8;
const REALLOC_INDEX: u32 = CHECK_COUNT;

fn mm(align: u32) -> MemArg {
    MemArg {
        offset: 0,
        align,
        memory_index: 0,
    }
}

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn realloc_call(asm: &mut Asm, old: i32, old_len: i32, align: i32, new_len: i32) {
    constant(asm, old);
    constant(asm, old_len);
    constant(asm, align);
    constant(asm, new_len);
    asm.leaf(I::Call(REALLOC_INDEX));
}

fn allocate_into(asm: &mut Asm, local: u32, n: i32, align: i32) {
    realloc_call(asm, 0, 0, align, n);
    set(asm, local);
}

fn free_call(asm: &mut Asm, pointer: u32, len: i32) {
    get(asm, pointer);
    constant(asm, len);
    constant(asm, 1);
    constant(asm, 0);
    asm.leaf(I::Call(REALLOC_INDEX));
    asm.leaf(I::Drop);
}

fn equal(asm: &mut Asm, left: u32, right: u32) {
    get(asm, left);
    get(asm, right);
    asm.leaf(I::I32Eq);
}

/// Allocates, frees, and confirms the freed block is reused and that two
/// adjacent freed blocks coalesce into one larger block.
fn check_reuse_and_coalesce() -> Body {
    let mut asm = Asm::new();
    allocate_into(&mut asm, 0, 16, 8);
    allocate_into(&mut asm, 1, 16, 8);
    // The two bump allocations are distinct.
    get(&mut asm, 0);
    get(&mut asm, 1);
    asm.leaf(I::I32Ne);

    free_call(&mut asm, 0, 16);
    allocate_into(&mut asm, 2, 16, 8);
    // The freed block is reused.
    equal(&mut asm, 2, 0);
    asm.leaf(I::I32And);

    free_call(&mut asm, 1, 16);
    free_call(&mut asm, 2, 16);
    allocate_into(&mut asm, 3, 32, 8);
    // The two adjacent freed blocks coalesced into the first one.
    equal(&mut asm, 3, 0);
    asm.leaf(I::I32And);
    asm.into_body()
}

/// Confirms alignment and in-place resize, then a grow-with-copy resize.
fn check_alignment_and_resize() -> Body {
    let mut asm = Asm::new();
    allocate_into(&mut asm, 0, 3, 1);
    for (offset, value) in [(0, 0x12), (1, 0x34), (2, 0x56)] {
        get(&mut asm, 0);
        if offset != 0 {
            constant(&mut asm, offset);
            asm.leaf(I::I32Add);
        }
        constant(&mut asm, value);
        asm.leaf(I::I32Store8(mm(0)));
    }

    // Seed the accumulator.
    constant(&mut asm, 1);

    // In-place resize within capacity keeps the pointer and the bytes.
    get(&mut asm, 0);
    constant(&mut asm, 3);
    constant(&mut asm, 16);
    constant(&mut asm, 5);
    asm.leaf(I::Call(REALLOC_INDEX));
    set(&mut asm, 1);
    get(&mut asm, 1);
    get(&mut asm, 0);
    asm.leaf(I::I32Eq);
    asm.leaf(I::I32And);
    get(&mut asm, 1);
    constant(&mut asm, 15);
    asm.leaf(I::I32And);
    asm.leaf(I::I32Eqz);
    asm.leaf(I::I32And);

    // Resize past capacity reallocates, copies the old bytes, and aligns.
    get(&mut asm, 1);
    constant(&mut asm, 5);
    constant(&mut asm, 8);
    constant(&mut asm, 32);
    asm.leaf(I::Call(REALLOC_INDEX));
    set(&mut asm, 2);
    get(&mut asm, 2);
    constant(&mut asm, 7);
    asm.leaf(I::I32And);
    asm.leaf(I::I32Eqz);
    asm.leaf(I::I32And);
    for (offset, expected) in [(0, 0x12), (1, 0x34), (2, 0x56)] {
        get(&mut asm, 2);
        if offset != 0 {
            constant(&mut asm, offset);
            asm.leaf(I::I32Add);
        }
        asm.leaf(I::I32Load8U(mm(0)));
        constant(&mut asm, expected);
        asm.leaf(I::I32Eq);
        asm.leaf(I::I32And);
    }
    // The length prefix records the resized length.
    get(&mut asm, 2);
    constant(&mut asm, 4);
    asm.leaf(I::I32Sub);
    asm.leaf(I::I32Load(mm(2)));
    constant(&mut asm, 32);
    asm.leaf(I::I32Eq);
    asm.leaf(I::I32And);
    asm.into_body()
}

fn bad_alignment() -> Body {
    let mut asm = Asm::new();
    realloc_call(&mut asm, 0, 0, 3, 1);
    asm.leaf(I::Drop);
    constant(&mut asm, 0);
    asm.into_body()
}

fn null_with_length() -> Body {
    let mut asm = Asm::new();
    realloc_call(&mut asm, 0, 7, 4, 8);
    asm.leaf(I::Drop);
    constant(&mut asm, 0);
    asm.into_body()
}

fn mismatched_length() -> Body {
    let mut asm = Asm::new();
    allocate_into(&mut asm, 0, 8, 4);
    get(&mut asm, 0);
    constant(&mut asm, 9);
    constant(&mut asm, 4);
    constant(&mut asm, 16);
    asm.leaf(I::Call(REALLOC_INDEX));
    asm.leaf(I::Drop);
    constant(&mut asm, 0);
    asm.into_body()
}

fn grow_failure() -> Body {
    let mut asm = Asm::new();
    // Exceeds the memory maximum, so memory.grow returns -1.
    realloc_call(&mut asm, 0, 0, 4, 200_000);
    asm.leaf(I::Drop);
    constant(&mut asm, 0);
    asm.into_body()
}

fn address_overflow() -> Body {
    let mut asm = Asm::new();
    // Force the bump break to the top of the wasm32 address space.
    constant(&mut asm, (abi::HEAP_STATE + 4) as i32);
    constant(&mut asm, -8);
    asm.leaf(I::I32Store(mm(2)));
    realloc_call(&mut asm, 0, 0, 4, 16);
    asm.leaf(I::Drop);
    constant(&mut asm, 0);
    asm.into_body()
}

/// Allocates and frees in a loop far past one page's worth of blocks, then
/// reports whether linear memory grew. Reclamation keeps it at one page.
fn check_bounded_growth() -> Body {
    let mut asm = Asm::new();
    constant(&mut asm, 10_000);
    set(&mut asm, 1);
    let done = asm.label();
    let again = asm.label();
    asm.block(done);
    asm.loop_(again);
    get(&mut asm, 1);
    asm.leaf(I::I32Eqz);
    asm.br_if(done);
    allocate_into(&mut asm, 0, 16, 8);
    free_call(&mut asm, 0, 16);
    get(&mut asm, 1);
    constant(&mut asm, 1);
    asm.leaf(I::I32Sub);
    set(&mut asm, 1);
    asm.br(again);
    asm.end();
    asm.end();
    asm.leaf(I::MemorySize(0));
    constant(&mut asm, 1);
    asm.leaf(I::I32Eq);
    asm.into_body()
}

fn function(name: &str, body: Body, locals: usize) -> Function {
    Function {
        symbol: SymbolId::new(ModuleId(0), 0),
        name: name.into(),
        type_index: TypeIndex(0),
        parameters: Vec::new(),
        locals: vec![ValType::I32; locals],
        body,
        span: span(),
    }
}

fn fixture() -> Module {
    let mut functions = vec![
        function("check_reuse_and_coalesce", check_reuse_and_coalesce(), 4),
        function(
            "check_alignment_and_resize",
            check_alignment_and_resize(),
            3,
        ),
        function("bad_alignment", bad_alignment(), 0),
        function("null_with_length", null_with_length(), 0),
        function("mismatched_length", mismatched_length(), 1),
        function("grow_failure", grow_failure(), 0),
        function("address_overflow", address_overflow(), 0),
        function("check_bounded_growth", check_bounded_growth(), 2),
    ];
    let realloc_type = TypeIndex(1);
    functions.push(build_realloc(realloc_type, span()));

    let mut state = 0_u32.to_le_bytes().to_vec();
    state.extend_from_slice(&abi::HEAP_START.to_le_bytes());

    let mut exports: Vec<Export> = functions
        .iter()
        .enumerate()
        .map(|(index, function)| Export {
            name: function.name.clone(),
            kind: ExportKind::Function,
            index: ExportIndex::Function(FunctionIndex(index as u32)),
        })
        .collect();
    exports.push(Export {
        name: "memory".into(),
        kind: ExportKind::Memory,
        index: ExportIndex::Memory(MemoryIndex(0)),
    });

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
                offset: abi::HEAP_STATE,
            },
            bytes: state,
        }],
        exports,
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: Vec::new(),
        span: span(),
    }
}

fn require_wasmtime() -> bool {
    match std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
    {
        Ok(_) => true,
        Err(_) if std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1") => {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
        }
        Err(_) => {
            eprintln!("skipping allocator execution: wasmtime is not installed");
            false
        }
    }
}

#[test]
fn realloc_reclaims_reuses_coalesces_aligns_and_traps() {
    let module = fixture();
    super::super::super::verify::verify_module(&module).expect("thin Wasm module verifies");
    let binary = super::super::super::encode_module(&module).expect("Wasm encodes");
    crate::validator()
        .validate_all(&binary)
        .expect("allocator module validates");

    if !require_wasmtime() {
        return;
    }

    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("psrs-realloc-{}-{id}.wasm", std::process::id()));
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

    for export in [
        "check_reuse_and_coalesce",
        "check_alignment_and_resize",
        "check_bounded_growth",
    ] {
        let output = run(export);
        assert!(output.status.success(), "{export} trapped: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "1",
            "{export} reported a failed check"
        );
    }
    for export in [
        "bad_alignment",
        "null_with_length",
        "mismatched_length",
        "grow_failure",
        "address_overflow",
    ] {
        let output = run(export);
        assert!(!output.status.success(), "{export} unexpectedly succeeded");
    }
    let _ = std::fs::remove_file(path);
}
