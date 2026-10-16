use super::*;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const IMPORTS: &str = r#"
  (type $array (array (mut eqref)))
  (type $box (struct (field i32)))
  (import "psrs:runtime-storage" "array_fill"
    (func $fill (param i32 eqref) (result (ref $array))))
  (import "psrs:runtime-storage" "array_read"
    (func $read (param (ref $array) i32) (result eqref)))
  (import "psrs:runtime-storage" "array_write"
    (func $write (param (ref $array) i32 eqref)))
  (import "psrs:runtime-storage" "trap" (func $trap))
"#;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn run(body: &str) -> Option<Output> {
    run_with_imports(IMPORTS, body)
}

fn run_with_imports(imports: &str, body: &str) -> Option<Output> {
    let version = Command::new("wasmtime").arg("--version").output();
    if !version.is_ok_and(|v| v.status.success()) {
        assert_ne!(
            std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(),
            Ok("1"),
            "required Wasmtime is unavailable"
        );
        eprintln!("skipping GC storage execution: Wasmtime is unavailable");
        return None;
    }
    let directory = std::env::temp_dir().join(format!(
        "psrs-runtime-storage-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir(&directory).unwrap();
    let provider = directory.join("storage.wasm");
    let guest = directory.join("guest.wat");
    std::fs::write(&provider, encode_module()).unwrap();
    std::fs::write(&guest, format!("(module {imports} {body})")).unwrap();
    let output = Command::new("wasmtime")
        .args(["run", "--preload"])
        .arg(format!("{}={}", crate::STORAGE_MODULE, provider.display()))
        .args(["--invoke", "test"])
        .arg(&guest)
        .output()
        .unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    Some(output)
}

#[test]
fn provider_validates_without_memory_or_state_storage() {
    let bytes = encode_module();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .unwrap();
    assert_eq!(bytes, encode_module());
    let mut exports = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        match payload.unwrap() {
            wasmparser::Payload::ExportSection(section) => {
                exports.extend(section.into_iter().map(|e| e.unwrap().name.to_owned()));
            }
            wasmparser::Payload::MemorySection(_)
            | wasmparser::Payload::GlobalSection(_)
            | wasmparser::Payload::ImportSection(_)
            | wasmparser::Payload::StartSection { .. } => {
                panic!("GC storage must not allocate state or linear-memory handles")
            }
            _ => {}
        }
    }
    assert_eq!(
        exports,
        StorageOperation::ALL.map(|operation| operation.abi().export)
    );
}

#[test]
fn writes_preserve_aliases_and_repeated_reads_observe_values() {
    let Some(output) = run(r#"
      (func (export "test") (result i32)
        (local $a (ref null $array)) (local $alias (ref null $array))
        (local $initial (ref null $box)) (local $updated (ref null $box))
        (local.set $initial (struct.new $box (i32.const 7)))
        (local.set $updated (struct.new $box (i32.const 19)))
        (local.set $a (call $fill (i32.const 2) (local.get $initial)))
        (local.set $alias (local.get $a))
        (if (i32.ne (array.len (local.get $a)) (i32.const 2))
          (then unreachable))
        (if (i32.eqz (ref.eq (call $read (ref.as_non_null (local.get $a))
              (i32.const 1)) (local.get $initial))) (then unreachable))
        (call $write (ref.as_non_null (local.get $a)) (i32.const 1)
          (local.get $updated))
        (if (i32.eqz (ref.eq (call $read (ref.as_non_null (local.get $alias))
              (i32.const 1)) (local.get $updated))) (then unreachable))
        (if (i32.ne (struct.get $box 0
              (ref.cast (ref $box) (call $read
                (ref.as_non_null (local.get $alias)) (i32.const 1))))
              (i32.const 19)) (then unreachable))
        (call $write (ref.as_non_null (local.get $alias)) (i32.const 1)
          (local.get $initial))
        (struct.get $box 0 (ref.cast (ref $box)
          (call $read (ref.as_non_null (local.get $a)) (i32.const 1)))))
    "#) else {
        return;
    };
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "7");
}

#[test]
fn zero_length_and_null_values_are_supported() {
    let Some(output) = run(r#"
      (func (export "test") (result i32)
        (if (i32.ne (array.len (call $fill (i32.const 0) (ref.null eq)))
              (i32.const 0)) (then unreachable))
        (ref.is_null (call $read (call $fill (i32.const 1) (ref.null eq))
          (i32.const 0))))
    "#) else {
        return;
    };
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1");
}

#[test]
fn incompatible_gc_array_import_is_rejected_before_execution() {
    // An immutable array is not the mutable storage representation even when
    // its element type agrees. Do not adapt an in-place write by copying it.
    let imports = IMPORTS.replace("(array (mut eqref))", "(array eqref)");
    let Some(output) = run_with_imports(
        &imports,
        "(func (export \"test\") (result i32) (i32.const 123))",
    ) else {
        return;
    };
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("incompatible import type"), "{stderr}");
    assert!(output.stdout.is_empty());
}

#[test]
fn invalid_sizes_indices_and_explicit_trap_have_no_normal_return() {
    for operation in [
        "(drop (call $fill (i32.const -1) (ref.null eq)))",
        "(drop (call $read (call $fill (i32.const 0) (ref.null eq)) (i32.const 0)))",
        "(drop (call $read (call $fill (i32.const 1) (ref.null eq)) (i32.const -1)))",
        "(call $write (call $fill (i32.const 1) (ref.null eq)) (i32.const 1) (ref.null eq))",
        "(call $trap)",
    ] {
        let Some(output) = run(&format!(
            "(func (export \"test\") (result i32) {operation} (i32.const 123))"
        )) else {
            return;
        };
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("wasm trap"), "{stderr}");
        assert!(
            output.stdout.is_empty(),
            "a trap must not return a fabricated payload"
        );
    }
}
