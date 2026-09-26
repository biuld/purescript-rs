//! Direct execution tests for the UTF-16 <-> UTF-8 codec.
//!
//! Each test builds a minimal module that decodes an active data segment of
//! UTF-8 bytes with `bytes_to_string` and exports the decoded code-unit count
//! and its first two code units. The module is validated and executed with
//! Wasmtime; the expected code units follow the WHATWG `TextDecoder` rules.

use super::decode::{bytes_to_string, decode_step};
use crate::types::{
    CompositeType, DataId, DefinedType, DefinedTypeId, FieldType, MemoryId, RecGroup, StorageType,
};
use crate::wasm::{
    DataIndex, DataMode, DataSegment, Export, ExportIndex, ExportKind, FuncType, Function,
    FunctionIndex, Memory, MemoryIndex, Module, Op, TypeIndex,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use wasm_encoder::{HeapType, Instruction, RefType as WasmRefType, ValType};

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn string_ref() -> ValType {
    ValType::Ref(WasmRefType {
        nullable: false,
        heap_type: HeapType::Concrete(0),
    })
}

fn export(name: &str, index: u32) -> Export {
    Export {
        name: name.into(),
        kind: ExportKind::Function,
        index: ExportIndex::Function(FunctionIndex(index)),
    }
}

/// Builds a module decoding `bytes` (at address 0) into a GC string. Exports
/// `decoded_units`, `first_unit`, and `second_unit`.
fn decode_module(bytes: &[u8]) -> Module {
    let string_type = DefinedTypeId(0);
    // Function 0..=2 are the harness; with no entry or realloc, the helpers
    // start at index 3 (`bytes_to_string`) and 4 (`decode_step`).
    let bytes_to_string_index = FunctionIndex(3);
    let decode_step_index = FunctionIndex(4);

    let harness = |name: &str, symbol: u32, unit: Option<u32>| Function {
        symbol: SymbolId::new(ModuleId(0), symbol),
        name: name.into(),
        type_index: TypeIndex(1),
        parameters: Vec::new(),
        locals: Vec::new(),
        body: {
            let mut body = vec![
                Op::Leaf(Instruction::I32Const(0)),
                Op::Leaf(Instruction::I32Const(bytes.len() as i32)),
                Op::Leaf(Instruction::Call(bytes_to_string_index.0)),
            ];
            match unit {
                None => body.push(Op::Leaf(Instruction::ArrayLen)),
                Some(index) => {
                    body.push(Op::Leaf(Instruction::I32Const(index as i32)));
                    body.push(Op::Leaf(Instruction::ArrayGetU(0)));
                }
            }
            body
        },
        span: span(),
    };

    Module {
        name: "codec-test".into(),
        imports: Vec::new(),
        types: vec![
            FuncType {
                parameters: Vec::new(),
                results: vec![ValType::I32],
            },
            FuncType {
                parameters: vec![ValType::I32, ValType::I32],
                results: vec![string_ref()],
            },
            FuncType {
                parameters: vec![ValType::I32, ValType::I32],
                results: vec![ValType::I32],
            },
        ],
        type_defs: vec![RecGroup(vec![DefinedType {
            final_type: true,
            supertype: None,
            composite: CompositeType::Array(FieldType {
                storage: StorageType::I16,
                mutable: true,
            }),
        }])],
        functions: vec![
            harness("decoded_units", 0, None),
            harness("first_unit", 1, Some(0)),
            harness("second_unit", 2, Some(1)),
        ],
        memories: vec![Memory {
            id: MemoryId(0),
            index: MemoryIndex(0),
            minimum: 1,
            maximum: None,
        }],
        globals: Vec::new(),
        data: vec![DataSegment {
            id: DataId(0),
            index: DataIndex(0),
            mode: DataMode::Active { offset: 0 },
            bytes: bytes.to_vec(),
        }],
        exports: vec![
            export("decoded_units", 0),
            export("first_unit", 1),
            export("second_unit", 2),
        ],
        entry: None,
        realloc: None,
        helpers: vec![
            bytes_to_string(string_type, decode_step_index, TypeIndex(2), span()),
            decode_step(TypeIndex(3), span()),
        ],
        span: span(),
    }
}

fn invoke(module: &Module, name: &str) -> Option<i32> {
    let available = std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available {
        if std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1") {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
        }
        eprintln!("skipping: wasmtime is not installed");
        return None;
    }
    crate::wasm::verify::verify_module(module).expect("the codec test module should verify");
    let binary =
        crate::wasm::encode_module(module).expect("the codec test module should encode to Wasm");
    crate::validator()
        .validate_all(&binary)
        .expect("the codec test module should validate");
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "psrs-codec-{}-{id}-{name}.wasm",
        std::process::id()
    ));
    std::fs::write(&path, &binary).expect("writing the codec test module");
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg("--invoke")
        .arg(name)
        .arg(&path)
        .output()
        .expect("running the codec test module");
    let _ = std::fs::remove_file(&path);
    assert!(output.status.success(), "wasmtime failed: {output:?}");
    Some(
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .expect("the export returns an i32"),
    )
}

fn assert_decodes(bytes: &[u8], units: i32, first: i32, second: Option<i32>) {
    let module = decode_module(bytes);
    let Some(decoded) = invoke(&module, "decoded_units") else {
        return;
    };
    assert_eq!(decoded, units, "code-unit count for {bytes:?}");
    if units > 0 {
        assert_eq!(invoke(&module, "first_unit"), Some(first), "first unit");
    }
    if units > 1
        && let Some(second) = second
    {
        assert_eq!(invoke(&module, "second_unit"), Some(second), "second unit");
    }
}

const REPLACEMENT: i32 = 0xFFFD;

#[test]
fn decodes_empty_ascii_bmp_and_astral() {
    assert_decodes(b"", 0, 0, None);
    assert_decodes(b"A", 1, 0x41, None);
    // "é" U+00E9
    assert_decodes(&[0xC3, 0xA9], 1, 0xE9, None);
    // "λ" U+03BB
    assert_decodes(&[0xCE, 0xBB], 1, 0x3BB, None);
    // U+1F600 encodes as a UTF-16 surrogate pair.
    assert_decodes(&[0xF0, 0x9F, 0x98, 0x80], 2, 0xD83D, Some(0xDE00));
}

#[test]
fn decodes_invalid_utf8_as_replacement() {
    // A lone continuation byte.
    assert_decodes(&[0x80], 1, REPLACEMENT, None);
    // 0xC0 is below the shortest two-byte lead.
    assert_decodes(&[0xC0], 1, REPLACEMENT, None);
    // 0xFF is an invalid lead byte.
    assert_decodes(&[0xFF], 1, REPLACEMENT, None);
    // A truncated two-byte sequence.
    assert_decodes(&[0xC3], 1, REPLACEMENT, None);
    // An encoded surrogate (CESU-8) is invalid and must not decode to a
    // surrogate code unit.
    let surrogate = decode_module(&[0xED, 0xA0, 0x80]);
    assert!(invoke(&surrogate, "decoded_units").is_some_and(|units| units >= 1));
    assert_eq!(invoke(&surrogate, "first_unit"), Some(REPLACEMENT));
    // Invalid bytes mixed with valid ASCII keep the ASCII.
    let module = decode_module(&[0xFF, 0x41]);
    assert_eq!(invoke(&module, "second_unit"), Some(0x41));
}
