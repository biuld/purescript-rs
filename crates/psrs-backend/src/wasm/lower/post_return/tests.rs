use super::super::super::{
    DataIndex, DataMode, DataSegment, Export, ExportIndex, ExportKind, FuncType, Function,
    FunctionIndex, Import, Memory, MemoryIndex, Module, Op, TypeIndex,
};
use super::super::asm::{Asm, constant, get, memarg, set};
use super::{
    BufferExport, post_return_name, synthesize_buffer_post_return,
    synthesize_owned_handle_post_return,
};
use crate::component::componentize;
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};
use wit_parser::{Resolve, WorldId, WorldItem};

fn span() -> TextRange {
    TextRange::new(0, 1)
}

#[test]
fn post_return_drops_an_owned_export_handle() {
    let wit = r#"
            package fixture:handles@0.1.0;
            interface types {
                resource thing;
            }
            world guest {
                import types;
                use types.{thing};
                export take: func() -> thing;
            }
        "#;
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str("handles.wit", wit)
        .expect("the handle fixture should resolve");
    let world = resolve.packages[package]
        .worlds
        .get("guest")
        .copied()
        .expect("guest world");
    let export = match &resolve.worlds[world].exports.values().next() {
        Some(WorldItem::Function(function)) => function.name.clone(),
        other => panic!("expected one function export, found {other:?}"),
    };
    assert_eq!(export, "take");
    assert_eq!(post_return_name(&export), "cabi_post_take");

    let drop_type = TypeIndex(0);
    let take_type = TypeIndex(1);
    let post_type = TypeIndex(0);
    let (post_signature, post_function, post_export) = synthesize_owned_handle_post_return(
        &export,
        post_type,
        FunctionIndex(2),
        FunctionIndex(0),
        SymbolId::new(ModuleId(0), 1),
        span(),
    );
    assert_eq!(post_signature.parameters, vec![ValType::I32]);
    assert!(post_signature.results.is_empty());
    assert_eq!(post_export.name, "cabi_post_take");

    let module = Module {
        name: "Handles".into(),
        imports: vec![Import {
            module: "fixture:handles/types@0.1.0".into(),
            name: "[resource-drop]thing".into(),
            type_index: drop_type,
        }],
        types: vec![
            FuncType {
                parameters: vec![ValType::I32],
                results: Vec::new(),
            },
            FuncType {
                parameters: Vec::new(),
                results: vec![ValType::I32],
            },
        ],
        type_defs: Vec::new(),
        functions: vec![
            Function {
                symbol: SymbolId::new(ModuleId(0), 0),
                name: export.clone(),
                type_index: take_type,
                parameters: Vec::new(),
                locals: Vec::new(),
                body: vec![Op::Leaf(Instruction::I32Const(1))],
                span: span(),
            },
            post_function,
        ],
        memories: vec![Memory {
            id: crate::types::MemoryId(0),
            index: MemoryIndex(0),
            minimum: 1,
            maximum: None,
        }],
        data: Vec::new(),
        exports: vec![
            Export {
                name: export,
                kind: ExportKind::Function,
                index: ExportIndex::Function(FunctionIndex(1)),
            },
            post_export,
            Export {
                name: "memory".into(),
                kind: ExportKind::Memory,
                index: ExportIndex::Memory(MemoryIndex(0)),
            },
        ],
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: Vec::new(),
        span: span(),
    };
    let core = crate::wasm::encode_module(&module).expect("encoding the core module");
    let component = componentize(&core, &resolve, world).expect("componentizing post-return");
    crate::validator()
        .validate_all(&component)
        .expect("the component should validate");
    let text = wasmprinter::print_bytes(&component).expect("printing the component");
    assert!(
        text.contains("resource.drop"),
        "post-return should lower to canon resource.drop: {text}"
    );
    assert!(
        text.contains("cabi_post_take") || text.contains("post-return"),
        "the export should have a post-return: {text}"
    );
}

const GET_STRING_INDEX: u32 = 0;
const POST_RETURN_INDEX: u32 = 1;
const REALLOC_INDEX: u32 = 2;
const DRIVER_INDEX: u32 = 3;

const GET_STRING_TYPE: TypeIndex = TypeIndex(0);
const POST_RETURN_TYPE: TypeIndex = TypeIndex(1);
const REALLOC_TYPE: TypeIndex = TypeIndex(2);

const STRING_LEN: i32 = 5;
const RETURN_AREA_SIZE: u32 = 8;
const RETURN_AREA_ALIGN: u32 = 4;
const CYCLES: i32 = 1_000;

fn call_realloc(asm: &mut Asm, old: i32, old_len: i32, align: i32, new_len: i32) {
    constant(asm, old);
    constant(asm, old_len);
    constant(asm, align);
    constant(asm, new_len);
    asm.leaf(Instruction::Call(REALLOC_INDEX));
}

/// Allocates the string buffer and the return area, writes `hello` and the
/// `(pointer, length)` pair, and returns the return-area pointer. This is
/// the shape of an export lifted through a canonical return area.
fn get_string_body() -> super::super::super::Body {
    let mut asm = Asm::new();
    call_realloc(&mut asm, 0, 0, 1, STRING_LEN);
    set(&mut asm, 0);
    for (offset, byte) in b"hello".iter().enumerate() {
        get(&mut asm, 0);
        if offset != 0 {
            constant(&mut asm, offset as i32);
            asm.leaf(Instruction::I32Add);
        }
        constant(&mut asm, i32::from(*byte));
        asm.leaf(Instruction::I32Store8(memarg(0)));
    }
    call_realloc(
        &mut asm,
        0,
        0,
        RETURN_AREA_ALIGN as i32,
        RETURN_AREA_SIZE as i32,
    );
    set(&mut asm, 1);
    get(&mut asm, 1);
    get(&mut asm, 0);
    asm.leaf(Instruction::I32Store(memarg(0)));
    get(&mut asm, 1);
    constant(&mut asm, STRING_LEN);
    asm.leaf(Instruction::I32Store(memarg(4)));
    get(&mut asm, 1);
    asm.into_body()
}

/// Calls the export and then its post-return `CYCLES` times, and reports
/// whether linear memory stayed at one page. Reclaimed buffers are reused.
fn driver_body() -> super::super::super::Body {
    let mut asm = Asm::new();
    constant(&mut asm, CYCLES);
    set(&mut asm, 0);
    let done = asm.label();
    let again = asm.label();
    asm.block(done);
    asm.loop_(again);
    get(&mut asm, 0);
    asm.leaf(Instruction::I32Eqz);
    asm.br_if(done);
    asm.leaf(Instruction::Call(GET_STRING_INDEX));
    set(&mut asm, 1);
    get(&mut asm, 1);
    asm.leaf(Instruction::Call(POST_RETURN_INDEX));
    get(&mut asm, 0);
    constant(&mut asm, 1);
    asm.leaf(Instruction::I32Sub);
    set(&mut asm, 0);
    asm.br(again);
    asm.end();
    asm.end();
    asm.leaf(Instruction::MemorySize(0));
    constant(&mut asm, 1);
    asm.leaf(Instruction::I32Eq);
    asm.into_body()
}

fn function(
    name: &str,
    symbol: u32,
    type_index: TypeIndex,
    locals: usize,
    body: Vec<Op>,
) -> Function {
    Function {
        symbol: SymbolId::new(ModuleId(0), symbol),
        name: name.into(),
        type_index,
        parameters: Vec::new(),
        locals: vec![ValType::I32; locals],
        body,
        span: span(),
    }
}

/// A core module exporting `get-string` (string result), its synthesized
/// `cabi_post_get-string`, and a driver that runs the pair in a loop.
fn buffer_post_return_module() -> Module {
    let (_, post_function, post_export) = synthesize_buffer_post_return(
        &BufferExport {
            core_name: "get-string".into(),
            symbol: SymbolId::new(ModuleId(0), 1),
            buffer_align: 1,
            return_area_size: RETURN_AREA_SIZE,
            return_area_align: RETURN_AREA_ALIGN,
        },
        POST_RETURN_TYPE,
        FunctionIndex(POST_RETURN_INDEX),
        FunctionIndex(REALLOC_INDEX),
        span(),
    );
    let mut state = 0_u32.to_le_bytes().to_vec();
    state.extend_from_slice(&crate::abi::HEAP_START.to_le_bytes());
    Module {
        name: "StringExport".into(),
        imports: Vec::new(),
        types: vec![
            FuncType {
                parameters: Vec::new(),
                results: vec![ValType::I32],
            },
            FuncType {
                parameters: vec![ValType::I32],
                results: Vec::new(),
            },
            FuncType {
                parameters: vec![ValType::I32; 4],
                results: vec![ValType::I32],
            },
        ],
        type_defs: Vec::new(),
        functions: vec![
            function(
                "get-string",
                GET_STRING_INDEX,
                GET_STRING_TYPE,
                2,
                get_string_body(),
            ),
            post_function,
            super::super::realloc::build_realloc(REALLOC_TYPE, span()),
            function(
                "check_post_return_reclaims",
                DRIVER_INDEX,
                GET_STRING_TYPE,
                2,
                driver_body(),
            ),
        ],
        memories: vec![Memory {
            id: crate::types::MemoryId(0),
            index: MemoryIndex(0),
            minimum: 1,
            maximum: Some(2),
        }],
        data: vec![DataSegment {
            id: crate::types::DataId(0),
            index: DataIndex(0),
            mode: DataMode::Active {
                offset: crate::abi::HEAP_STATE,
            },
            bytes: state,
        }],
        exports: vec![
            Export {
                name: "get-string".into(),
                kind: ExportKind::Function,
                index: ExportIndex::Function(FunctionIndex(GET_STRING_INDEX)),
            },
            post_export,
            Export {
                name: "cabi_realloc".into(),
                kind: ExportKind::Function,
                index: ExportIndex::Function(FunctionIndex(REALLOC_INDEX)),
            },
            Export {
                name: "check_post_return_reclaims".into(),
                kind: ExportKind::Function,
                index: ExportIndex::Function(FunctionIndex(DRIVER_INDEX)),
            },
            Export {
                name: "memory".into(),
                kind: ExportKind::Memory,
                index: ExportIndex::Memory(MemoryIndex(0)),
            },
        ],
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: Vec::new(),
        span: span(),
    }
}

fn string_world() -> (Resolve, WorldId) {
    let wit = r#"
            package fixture:strings@0.1.0;
            world guest {
                export get-string: func() -> string;
            }
        "#;
    let mut resolve = Resolve::default();
    let package = resolve
        .push_str("strings.wit", wit)
        .expect("the string fixture should resolve");
    let world = resolve.packages[package]
        .worlds
        .get("guest")
        .copied()
        .expect("guest world");
    (resolve, world)
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
            eprintln!("skipping post-return execution: wasmtime is not installed");
            false
        }
    }
}

#[test]
fn buffer_post_return_frees_the_returned_string() {
    assert_eq!(post_return_name("get-string"), "cabi_post_get-string");
    let module = buffer_post_return_module();
    crate::wasm::verify::verify_module(&module).expect("thin Wasm module verifies");
    let binary = crate::wasm::encode_module(&module).expect("Wasm encodes");
    crate::validator()
        .validate_all(&binary)
        .expect("post-return module validates");

    if !require_wasmtime() {
        return;
    }

    let path = std::env::temp_dir().join(format!(
        "psrs-buffer-post-return-{}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, binary).expect("write post-return test module");
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg("--invoke")
        .arg("check_post_return_reclaims")
        .arg(&path)
        .output()
        .expect("run post-return test module");
    let _ = std::fs::remove_file(&path);
    assert!(output.status.success(), "driver trapped: {output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "1",
        "post-return did not reclaim the returned buffer"
    );
}

#[test]
fn component_attaches_the_buffer_post_return() {
    let module = buffer_post_return_module();
    let binary = crate::wasm::encode_module(&module).expect("Wasm encodes");
    let (resolve, world) = string_world();
    let component =
        componentize(&binary, &resolve, world).expect("componentizing the string export");
    crate::validator()
        .validate_all(&component)
        .expect("the component should validate");
    let text = wasmprinter::print_bytes(&component).expect("printing the component");
    assert!(
        text.contains("post-return"),
        "the component should attach a post-return: {text}"
    );

    if !require_wasmtime() {
        return;
    }
    let path = std::env::temp_dir().join(format!(
        "psrs-buffer-post-return-component-{}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, &component).expect("write post-return component");
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg("--invoke")
        .arg("get-string()")
        .arg(&path)
        .output()
        .expect("run post-return component");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "the component export failed: {output:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("hello"),
        "the component should lift the string: {output:?}"
    );
}
