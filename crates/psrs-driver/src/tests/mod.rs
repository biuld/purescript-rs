use super::*;
use crate::program::lower_program_to_core;

mod effects;
mod scalars;

fn lower_source_to_mir(source: &str) -> psrs_backend::mir::Module {
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    let backend_input = psrs_backend::cc::lower_module(core).expect("Core should lower to CC");
    psrs_backend::mir::lower_module_with_bindings(
        backend_input.cc,
        backend_input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .expect("CC should lower to MIR")
    .0
}

fn run_with_wasmtime(source: &str) -> Option<std::process::Output> {
    if std::process::Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_err()
    {
        if std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1") {
            panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not installed");
        }
        return None;
    }
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let artifact = compile_source("Main.purs", source).unwrap();
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("psrs-{}-{id}.wasm", std::process::id()));
    std::fs::write(&path, &artifact.wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Some(output)
}

mod backend;
mod declarations;
mod integration;
mod kinds;
mod resolution;
mod typecheck;
mod wasi;

mod adts;

mod pattern_matching_audit;

mod arrays;

mod records;

mod functions;

mod module_loader;

mod library_types;

mod tail_calls;
