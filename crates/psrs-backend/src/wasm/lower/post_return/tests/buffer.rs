//! String-export fixture whose `cabi_realloc` forwards to the allocator unit.

use super::super::super::asm::{Asm, constant, get, memarg, set};
use super::super::{BufferExport, synthesize_buffer_post_return};
use crate::wasm::{
    Body, Export, ExportIndex, ExportKind, FuncType, Function, FunctionIndex, Import, Memory,
    MemoryIndex, Module, Op, TypeIndex,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{Instruction, ValType};
use wit_parser::{Resolve, WorldId};

const IMPORT_REALLOC: u32 = 0;
const GET_STRING_INDEX: u32 = 1;
const POST_RETURN_INDEX: u32 = 2;
const REALLOC_INDEX: u32 = 3;
const DRIVER_INDEX: u32 = 4;

const GET_STRING_TYPE: TypeIndex = TypeIndex(0);
const POST_RETURN_TYPE: TypeIndex = TypeIndex(1);
const REALLOC_TYPE: TypeIndex = TypeIndex(2);

const STRING_LEN: i32 = 5;
const RETURN_AREA_SIZE: u32 = 8;
const RETURN_AREA_ALIGN: u32 = 4;
const CYCLES: i32 = 1_000;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn call_realloc(asm: &mut Asm, old: i32, old_len: i32, align: i32, new_len: i32) {
    constant(asm, old);
    constant(asm, old_len);
    constant(asm, align);
    constant(asm, new_len);
    asm.leaf(Instruction::Call(REALLOC_INDEX));
}

fn get_string_body() -> Body {
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

fn driver_body() -> Body {
    let mut asm = Asm::new();
    asm.leaf(Instruction::Call(GET_STRING_INDEX));
    set(&mut asm, 2);
    get(&mut asm, 2);
    asm.leaf(Instruction::Call(POST_RETURN_INDEX));
    asm.leaf(Instruction::MemorySize(0));
    set(&mut asm, 1);
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
    set(&mut asm, 2);
    get(&mut asm, 2);
    asm.leaf(Instruction::Call(POST_RETURN_INDEX));
    get(&mut asm, 0);
    constant(&mut asm, 1);
    asm.leaf(Instruction::I32Sub);
    set(&mut asm, 0);
    asm.br(again);
    asm.end();
    asm.end();
    asm.leaf(Instruction::MemorySize(0));
    get(&mut asm, 1);
    asm.leaf(Instruction::I32Eq);
    asm.into_body()
}

fn forward_realloc() -> Function {
    Function {
        symbol: SymbolId::new(ModuleId(0), REALLOC_INDEX),
        name: psrs_runtime::REALLOC_EXPORT.into(),
        type_index: REALLOC_TYPE,
        parameters: vec![ValType::I32; 4],
        locals: Vec::new(),
        body: vec![
            Op::Leaf(Instruction::LocalGet(0)),
            Op::Leaf(Instruction::LocalGet(1)),
            Op::Leaf(Instruction::LocalGet(2)),
            Op::Leaf(Instruction::LocalGet(3)),
            Op::Leaf(Instruction::Call(IMPORT_REALLOC)),
        ],
        span: span(),
    }
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

/// A core module exporting `get-string`, its post-return, and a reclaim driver.
pub(super) fn buffer_post_return_module() -> Module {
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
    Module {
        name: "StringExport".into(),
        imports: vec![Import {
            module: psrs_runtime::ALLOCATOR_MODULE.into(),
            name: psrs_runtime::REALLOC_EXPORT.into(),
            type_index: REALLOC_TYPE,
        }],
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
            forward_realloc(),
            function(
                "check-reclaim",
                DRIVER_INDEX,
                GET_STRING_TYPE,
                3,
                driver_body(),
            ),
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
                name: "get-string".into(),
                kind: ExportKind::Function,
                index: ExportIndex::Function(FunctionIndex(GET_STRING_INDEX)),
            },
            post_export,
            Export {
                name: psrs_runtime::REALLOC_EXPORT.into(),
                kind: ExportKind::Function,
                index: ExportIndex::Function(FunctionIndex(REALLOC_INDEX)),
            },
            Export {
                name: "check-reclaim".into(),
                kind: ExportKind::Function,
                index: ExportIndex::Function(FunctionIndex(DRIVER_INDEX)),
            },
            Export {
                name: "memory".into(),
                kind: ExportKind::Memory,
                index: ExportIndex::Memory(MemoryIndex(0)),
            },
            Export {
                name: psrs_runtime::HEAP_BOUNDARY_IMPORT.into(),
                kind: ExportKind::Function,
                index: ExportIndex::Function(FunctionIndex(DRIVER_INDEX + 1)),
            },
        ],
        entry: None,
        realloc: None,
        globals: Vec::new(),
        helpers: vec![crate::wasm::lower::realloc::heap_getter(
            GET_STRING_TYPE,
            0,
            span(),
        )],
        span: span(),
    }
}

pub(super) fn string_world() -> (Resolve, WorldId) {
    let wit = r#"
            package fixture:strings@0.1.0;
            world guest {
                export get-string: func() -> string;
                export check-reclaim: func() -> s32;
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

pub(super) fn allocator_requirement() -> psrs_linker::BindingRequirement {
    psrs_linker::BindingRequirement {
        id: psrs_linker::RequirementId(0),
        origin: psrs_runtime::ALLOCATOR_REALLOC_OP.name.into(),
        boundary: psrs_linker::Boundary::RawCore {
            module: psrs_runtime::ALLOCATOR_MODULE.into(),
            field: psrs_runtime::REALLOC_EXPORT.into(),
        },
        expected: Some(psrs_linker::CoreSignature {
            parameters: vec![psrs_linker::CoreType::I32; 4],
            result: Some(psrs_linker::CoreType::I32),
        }),
        provider: psrs_linker::Provider::RuntimeOperation {
            name: psrs_runtime::ALLOCATOR_REALLOC_OP.name.into(),
            version: psrs_runtime::ALLOCATOR_REALLOC_OP.version.into(),
        },
    }
}
