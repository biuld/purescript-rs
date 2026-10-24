//! A forwarding table breaks the guest/host memory dependency at instantiation.
//! The pinned production allocator supplies actual allocation and free behavior.

use crate::{ExternalBindings, TargetCapabilities, abi, cc, wasm};
use std::fmt::Write;
use std::process::Command;
use wasm_encoder::ValType;
use wit_parser::Resolve;

fn core_module(name: &str, bytes: &[u8]) -> String {
    wasmprinter::print_bytes(bytes)
        .expect("print core module")
        .replacen("(module", &format!("(core module ${name}"), 1)
}

fn signature(ty: &wasm::FuncType) -> String {
    let scalar = |value| match value {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
        _ => panic!("canonical host signature must be scalar"),
    };
    let mut text = String::new();
    for value in &ty.parameters {
        write!(text, " (param {})", scalar(*value)).unwrap();
    }
    for value in &ty.results {
        write!(text, " (result {})", scalar(*value)).unwrap();
    }
    text
}

fn assemble(guest: &wasm::Module, host: &str) -> String {
    let bytes = wasm::encode_module(guest).expect("guest encodes");
    crate::validator()
        .validate_all(&bytes)
        .expect("guest validates");
    let mut shim = String::new();
    let mut install = String::new();
    let mut modules = std::collections::BTreeSet::new();
    for (index, import) in guest.imports.iter().enumerate() {
        assert!(
            guest.imports[..index]
                .iter()
                .all(|other| other.name != import.name)
        );
        let ty = &guest.types[(import.type_index.0 - guest.defined_type_count()) as usize];
        let sig = signature(ty);
        write!(shim, "(type $t{index} (func {sig}))").unwrap();
        write!(shim, "(func (export {:?}) {sig} ", import.name).unwrap();
        for parameter in 0..ty.parameters.len() {
            write!(shim, "local.get {parameter} ").unwrap();
        }
        write!(shim, "i32.const {index} call_indirect (type $t{index}))").unwrap();
        let provider = if import.module == psrs_runtime::ALLOCATOR_MODULE {
            "allocator"
        } else {
            "host"
        };
        write!(
            install,
            "(import {provider:?} {:?} (func $f{index} {sig}))",
            import.name
        )
        .unwrap();
        modules.insert(import.module.as_str());
    }
    install.push_str("(import \"shim\" \"table\" (table 1 funcref)) (elem (i32.const 0)");
    for index in 0..guest.imports.len() {
        write!(install, " $f{index}").unwrap();
    }
    install.push(')');
    let withs: String = modules
        .iter()
        .map(|name| format!("(with {name:?} (instance $shim))"))
        .collect();
    let needs_allocator = modules.contains(psrs_runtime::ALLOCATOR_MODULE);
    let allocator = if needs_allocator {
        format!(
            r#"{}
          (core instance $allocator (instantiate $allocator-module
            (with "env" (instance $guest))
            (with "__main_module__" (instance $guest))))"#,
            core_module("allocator-module", psrs_runtime::ALLOCATOR_RUNTIME.bytes)
        )
    } else {
        "(core instance $allocator)".into()
    };
    let allocator_import = if needs_allocator {
        r#"(import "allocator" "cabi_realloc" (func $alloc (param i32 i32 i32 i32) (result i32)))"#
    } else {
        ""
    };
    format!(
        r#"(component
      (core module $shim-module
        (table (export "table") {count} funcref) {shim})
      (core instance $shim (instantiate $shim-module))
      {guest_module}
      (core instance $guest (instantiate $guest-module {withs}))
      {allocator}
      (core module $host-module
        (import "guest" "memory" (memory 1))
        {allocator_import}
        {host})
      (core instance $host (instantiate $host-module
        (with "guest" (instance $guest)) (with "allocator" (instance $allocator))))
      (core module $install-module {install})
      (core instance $installed (instantiate $install-module
        (with "shim" (instance $shim)) (with "host" (instance $host))
        (with "allocator" (instance $allocator))))
      (func (export "check") (result s32)
        (canon lift (core func $guest {run:?}))))"#,
        count = guest.imports.len(),
        guest_module = core_module("guest-module", &bytes),
        run = abi::RUN_CORE_EXPORT,
    )
}

pub(super) fn execute(
    name: &str,
    module: cc::Module,
    bindings: ExternalBindings,
    resolve: Resolve,
    host: &str,
    expected: i32,
) {
    cc::verify_module(&module).expect("CC fixture verifies before ABI lowering");
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) =
        super::super::lower_module_with_registry(module, bindings, target, registry)
            .expect("CC fixture lowers");
    let mir = crate::mir::opt::optimize(mir, target).expect("MIR optimizes");
    let guest =
        wasm::lower_module_with_capabilities(&mir, &mut registry, target).expect("Wasm lowers");
    let component = assemble(&guest, host);
    let available = Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !available {
        assert_ne!(
            std::env::var("PSRS_REQUIRE_WASMTIME").as_deref(),
            Ok("1"),
            "Wasmtime is required"
        );
        eprintln!("skipping aggregate execution: Wasmtime is unavailable");
        return;
    }
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "psrs-aggregate-{}-{id}-{name}.wat",
        std::process::id()
    ));
    std::fs::write(&path, component).expect("write component");
    let output = Command::new("wasmtime")
        .args(["run", "--invoke", "check()"])
        .arg(&path)
        .output()
        .expect("execute component");
    assert!(
        output.status.success(),
        "{name}: {} (artifact {})",
        String::from_utf8_lossy(&output.stderr),
        path.display()
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        expected.to_string(),
        "{name}"
    );
    std::fs::remove_file(path).expect("remove component");
}
