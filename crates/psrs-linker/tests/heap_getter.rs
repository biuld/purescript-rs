//! Cross-module heap configuration and rejection of executable getter bodies.
use psrs_linker::{
    BindingRequirement, Boundary, CoreSignature, CoreType, MemoryDemand, Provider, RequirementId,
    TargetLinkInput, TargetPolicy, plan, resolve_default_definitions,
};

fn input(heap_start: u32) -> TargetLinkInput {
    TargetLinkInput {
        requirements: vec![BindingRequirement {
            id: RequirementId(0),
            origin: "allocator".into(),
            boundary: Boundary::RawCore {
                module: psrs_runtime::ALLOCATOR_MODULE.into(),
                field: psrs_runtime::REALLOC_EXPORT.into(),
            },
            expected: Some(CoreSignature {
                parameters: vec![CoreType::I32; 4],
                result: Some(CoreType::I32),
            }),
            provider: Provider::RuntimeOperation {
                name: psrs_runtime::ALLOCATOR_REALLOC_OP.name.into(),
                version: psrs_runtime::ALLOCATOR_REALLOC_OP.version.into(),
            },
        }],
        units: vec![psrs_linker::runtime::offer(&psrs_runtime::ALLOCATOR_UNIT)],
        policy: TargetPolicy::default(),
        memory: MemoryDemand {
            canonical_scratch: (0, 16384),
            allocator_state: (16384, 16384),
            base_heap_start: heap_start,
            heap_alignment: 8,
            growth_owner: psrs_runtime::ALLOCATOR_UNIT.id.into(),
            maximum_pages: None,
        },
    }
}

fn application(pages: u64, getter: &str, getter_export: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
          (import "psrs:allocator" "cabi_realloc"
            (func $alloc (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") {pages})
          (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
            local.get 0 local.get 1 local.get 2 local.get 3 call $alloc)
          (func (export "wasi:cli/run@0.2.12#run") (result i32) i32.const 0)
          (func (export "{getter_export}") {getter}))"#
    ))
    .unwrap()
}

#[test]
fn one_pinned_allocator_accepts_different_checked_heap_boundaries() {
    let context = resolve_default_definitions().unwrap();
    for heap in [65536, 262144] {
        let link = plan(&context, input(heap)).unwrap();
        assert_eq!(link.memory().heap_start, heap);
        let core = application(
            link.memory().minimum_pages,
            &format!("(result i32) i32.const {heap}"),
            psrs_runtime::HEAP_BOUNDARY_IMPORT,
        );
        let component = psrs_linker::compose(&context, &link, &core).unwrap();
        assert!(component.external_world.is_empty());
    }
}

#[test]
fn getter_must_be_a_local_constant_with_the_checked_signature_and_value() {
    let context = resolve_default_definitions().unwrap();
    let link = plan(&context, input(65536)).unwrap();
    for getter in [
        "(result i32) i32.const 65544",
        "(param i32) (result i32) i32.const 65536",
        "(result i64) i64.const 65536",
        "(result i32) (local i32) i32.const 65536",
        "(result i32) memory.size drop i32.const 65536",
        "(result i32) i32.const 1 i32.const 2 i32.store i32.const 65536",
        "(result i32) i32.const 0 i32.const 0 i32.const 8 i32.const 8 call $alloc drop i32.const 65536",
    ] {
        let core = application(
            link.memory().minimum_pages,
            getter,
            psrs_runtime::HEAP_BOUNDARY_IMPORT,
        );
        let error = psrs_linker::compose(&context, &link, &core)
            .unwrap_err()
            .to_string();
        assert!(error.contains("heap getter"), "{getter}: {error}");
    }
    let core = application(
        link.memory().minimum_pages,
        "(result i32) i32.const 65536",
        "other",
    );
    assert!(psrs_linker::compose(&context, &link, &core).is_err());
}

#[test]
fn allocator_import_signature_drift_is_rejected() {
    let mut contract = psrs_linker::runtime::contract(&psrs_runtime::ALLOCATOR_RUNTIME);
    let import = contract
        .imports
        .iter_mut()
        .find(|import| matches!(import.kind, psrs_linker::ImportKind::Function(_)))
        .unwrap();
    import.kind = psrs_linker::ImportKind::Function(CoreSignature {
        parameters: vec![CoreType::I32],
        result: Some(CoreType::I32),
    });
    assert!(
        psrs_linker::verify_artifact(&contract, psrs_runtime::ALLOCATOR_RUNTIME.bytes).is_err()
    );
}

#[test]
fn allocator_executes_with_each_application_supplied_boundary() {
    use std::process::Command;
    if Command::new("wasmtime").arg("--version").output().is_err() {
        assert_ne!(std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(), Ok("1"));
        eprintln!("skipping heap getter execution: wasmtime is not installed");
        return;
    }
    let context = resolve_default_definitions().unwrap();
    for heap in [65536, 262144] {
        let link = plan(&context, input(heap)).unwrap();
        // Allocate beyond the initial page, verify bounds and scratch survival,
        // resize with content preservation, then free through the same instance.
        let run = format!(
            r#"
          (func (export "wasi:cli/run@0.2.12#run") (result i32) (local $p i32)
            i32.const 32 i32.const 1234 i32.store
            i32.const 0 i32.const 0 i32.const 8 i32.const 131072 call 0 local.set $p
            local.get $p i32.const {heap} i32.lt_u if unreachable end
            local.get $p i32.const 42 i32.store
            i32.const 32 i32.load i32.const 1234 i32.ne if unreachable end
            local.get $p i32.const 131072 i32.const 8 i32.const 262144 call 0 local.set $p
            local.get $p i32.load i32.const 42 i32.ne if unreachable end
            local.get $p i32.const 262144 i32.const 8 i32.const 0 call 0 drop
            i32.const 0)
        "#
        );
        let core = wat::parse_str(format!(
            r#"(module
          (import "psrs:allocator" "cabi_realloc" (func (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") {})
          (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32)
            local.get 0 local.get 1 local.get 2 local.get 3 call 0)
          (func (export "get_heap_base") (result i32) i32.const {heap})
          {run})"#,
            link.memory().minimum_pages
        ))
        .unwrap();
        let component = psrs_linker::compose(&context, &link, &core).unwrap();
        let path = std::env::temp_dir().join(format!(
            "psrs-heap-getter-{}-{heap}.wasm",
            std::process::id()
        ));
        std::fs::write(&path, component.bytes).unwrap();
        let output = Command::new("wasmtime")
            .arg("run")
            .arg(&path)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(path);
        assert!(output.status.success(), "heap {heap}: {output:?}");
    }
}
