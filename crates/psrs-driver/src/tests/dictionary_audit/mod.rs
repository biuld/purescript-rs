//! Backend evidence for the Type Classes and Dictionaries topic (DICT-01..11).
//!
//! The source frontend does not yet produce class/instance evidence (FE-14/15),
//! so these fixtures start at the verified Typed Core boundary. They lower THIR
//! carrying explicit `Given`, `Global`, `Instance`, and `Superclass` evidence
//! through the normal backend pipeline and execute it with Wasmtime when it is
//! available. The source gate is tracked separately in the topic checklist.

use psrs_hir::SymbolId;
use psrs_thir as thir;

mod execution;
mod fixtures;
mod negative;

fn required_wasmtime() -> bool {
    std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1")
}

fn execute_wasm(name: &str, wasm: &[u8]) -> Result<Option<std::process::Output>, String> {
    let available = std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available {
        if required_wasmtime() {
            return Err(format!("wasmtime is unavailable for `{name}`"));
        }
        eprintln!("skipping `{name}`: wasmtime is not installed");
        return Ok(None);
    }
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("psrs-dictionary-{}-{id}.wasm", std::process::id()));
    std::fs::write(&path, wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Ok(Some(output))
}

/// Lowers a Typed Core fixture, sets its selected entry, and executes the
/// optimized artifact through Wasmtime.
fn expect_fixture_exit(name: &str, fixture: thir::Module, entry: SymbolId, expected: i32) {
    let stages = compile_fixture(name, fixture, entry);
    match execute_wasm(name, &stages.artifact.wasm) {
        Ok(Some(output)) => assert_eq!(
            output.status.code(),
            Some(expected),
            "`{name}` produced the wrong exit code; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        ),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

fn compile_fixture(name: &str, fixture: thir::Module, entry: SymbolId) -> psrs_backend::Stages {
    let mut core = psrs_core::lower_module(fixture)
        .unwrap_or_else(|errors| panic!("`{name}` Typed Core should lower: {errors:?}"));
    core.entry = Some(entry);
    core.verify()
        .unwrap_or_else(|errors| panic!("`{name}` Core should verify: {errors:?}"));
    psrs_backend::compile_with_stages(core)
        .unwrap_or_else(|errors| panic!("`{name}` should compile: {errors:?}"))
}
