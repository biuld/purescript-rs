//! Direct execution tests for the string boundary helpers.
//!
//! Each test builds a minimal module that copies an active data segment of
//! UTF-8 bytes into a GC string with `bytes_to_string` and exports the byte
//! count and its first two bytes. The module is validated and executed with
//! Wasmtime. Malformed text must reject the value rather than produce U+FFFD,
//! so those cases assert a trap.

use super::decode::{bytes_to_string, validate_step};
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

/// Builds a module copying `bytes` (at address 0) into a GC string. Exports
/// `decoded_bytes`, `first_byte`, and `second_byte`.
fn decode_module(bytes: &[u8]) -> Module {
    let string_type = DefinedTypeId(0);
    // Function 0..=2 are the harness; with no entry or realloc, the helpers
    // start at index 3 (`bytes_to_string`) and 4 (`validate_step`).
    let bytes_to_string_index = FunctionIndex(3);
    let validate_step_index = FunctionIndex(4);

    let harness = |name: &str, symbol: u32, byte: Option<u32>| Function {
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
            match byte {
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
                storage: StorageType::I8,
                mutable: true,
            }),
        }])],
        functions: vec![
            harness("decoded_bytes", 0, None),
            harness("first_byte", 1, Some(0)),
            harness("second_byte", 2, Some(1)),
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
            export("decoded_bytes", 0),
            export("first_byte", 1),
            export("second_byte", 2),
        ],
        entry: None,
        realloc: None,
        helpers: vec![
            bytes_to_string(string_type, validate_step_index, TypeIndex(2), span()),
            validate_step(TypeIndex(3), span()),
        ],
        span: span(),
    }
}

fn invoke_ok(module: &Module, name: &str) -> Option<i32> {
    let (success, stdout) = invoke(module, name)?;
    assert!(success, "wasmtime trapped running {name}: {stdout:?}");
    Some(stdout.trim().parse().expect("the export returns an i32"))
}

/// Runs the module and reports whether the call trapped, with its stdout.
fn invoke(module: &Module, name: &str) -> Option<(bool, String)> {
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
    Some((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    ))
}

fn assert_copies(bytes: &[u8], count: i32, first: i32, second: Option<i32>) {
    let module = decode_module(bytes);
    let Some(decoded) = invoke_ok(&module, "decoded_bytes") else {
        return;
    };
    assert_eq!(decoded, count, "byte count for {bytes:?}");
    if count > 0 {
        assert_eq!(invoke_ok(&module, "first_byte"), Some(first), "first byte");
    }
    if count > 1
        && let Some(second) = second
    {
        assert_eq!(
            invoke_ok(&module, "second_byte"),
            Some(second),
            "second byte"
        );
    }
}

#[test]
fn copies_valid_utf8_unchanged() {
    assert_copies(b"", 0, 0, None);
    assert_copies(b"A", 1, 0x41, None);
    // "é" U+00E9 is two UTF-8 bytes.
    assert_copies(&[0xC3, 0xA9], 2, 0xC3, Some(0xA9));
    // "λ" U+03BB.
    assert_copies(&[0xCE, 0xBB], 2, 0xCE, Some(0xBB));
    // U+1F600 is one four-byte sequence, stored as four bytes.
    assert_copies(&[0xF0, 0x9F, 0x98, 0x80], 4, 0xF0, Some(0x9F));
    // A supplementary scalar's bytes are copied, not a surrogate pair.
    assert_copies("𝌆".as_bytes(), 4, 0xF0, Some(0x9D));
}

#[test]
fn rejects_malformed_utf8_instead_of_replacing_it() {
    for bytes in [
        &[0x80u8][..],             // a lone continuation byte
        &[0xC0],                   // below the shortest two-byte lead
        &[0xFF],                   // not a lead byte
        &[0xC3],                   // a truncated two-byte sequence
        &[0xE2, 0x28, 0xA1],       // a bad continuation byte
        &[0xED, 0xA0, 0x80],       // CESU-8: an encoded surrogate
        &[0xF0, 0x9F, 0x98],       // a truncated four-byte sequence
        &[0xF5, 0x80, 0x80, 0x80], // above U+10FFFF
        &[0xC0, 0x80],             // an overlong NUL
        &[0x41, 0xFF],             // valid text followed by malformed bytes
    ] {
        let module = decode_module(bytes);
        let Some((success, stdout)) = invoke(&module, "decoded_bytes") else {
            continue;
        };
        assert!(
            !success,
            "{bytes:?} must be rejected, not decoded: {stdout:?}"
        );
    }
}
