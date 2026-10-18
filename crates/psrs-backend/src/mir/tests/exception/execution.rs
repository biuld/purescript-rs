//! Execution helper for the exception-channel evidence: lowers a MIR module
//! with an explicit target profile, validates the encoded binary, componentizes
//! it, and executes it with the wasmtime CLI. Mirrors `gc_tests`' runner, with
//! the target passed by the fixture (the stable default profile keeps
//! `exceptions` disabled; every fixture constructs an explicit enabled target).
use crate::TargetCapabilities;
use std::sync::atomic::{AtomicU32, Ordering};

pub(super) fn next_artifact_id() -> u32 {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Lowers, validates, componentizes, and executes a MIR module under the
/// wasmtime CLI, or returns `None` when wasmtime is unavailable and not
/// required.
pub(super) fn run_output(
    mir: &crate::mir::Module,
    target: TargetCapabilities,
) -> Option<std::process::Output> {
    let mut wasi = crate::abi::WasiRegistry::load_with_capabilities(target)
        .expect("the vendored WASI WIT should load");
    let context = crate::linking::default_context().expect("default target context");
    let link = crate::linking::plan_for_module(&context, mir, &mut wasi, target)
        .expect("checked target plan");
    let wasm = crate::wasm::lower_module_with_plan(mir, &mut wasi, target, &link)
        .expect("the MIR should lower to Wasm");
    let core = crate::wasm::encode_module(&wasm).expect("encoding Wasm");
    crate::validator_for(target)
        .validate_all(&core)
        .expect("the module should validate");
    let component =
        crate::linking::compose(&link, &core, mir.span, mir.entry.map(|entry| entry.module))
            .expect("componentizing the module");
    let path = std::env::temp_dir().join(format!(
        "psrs-exception-{}-{}.wasm",
        std::process::id(),
        next_artifact_id()
    ));
    std::fs::write(&path, &component).expect("writing component");
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .expect("running component");
    let _ = std::fs::remove_file(&path);
    Some(output)
}

/// Panics when `PSRS_REQUIRE_WASMTIME=1` but the engine is unusable; returns
/// `false` when a skip is allowed.
pub(super) fn require_engine() -> bool {
    let required = std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1");
    let version = std::process::Command::new("wasmtime")
        .arg("--version")
        .output();
    match version {
        Ok(version) if version.status.success() => true,
        version => {
            if required {
                panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not usable: {version:?}");
            }
            eprintln!("skipping: wasmtime is not usable");
            false
        }
    }
}
