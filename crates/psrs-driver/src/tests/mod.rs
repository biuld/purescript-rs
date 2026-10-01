use super::*;
use crate::program::lower_program_to_core;
use std::sync::atomic::{AtomicU32, Ordering};

static WASM_ARTIFACT_COUNTER: AtomicU32 = AtomicU32::new(0);

mod coercion;
mod deriving;
mod effect_arity;
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
    run_with_wasmtime_args(source, &[])
}

/// Runs a compiled multi-module program under Wasmtime. Returns `None` when
/// Wasmtime is unavailable, unless `PSRS_REQUIRE_WASMTIME` is set, in which
/// case a missing toolchain is a failure rather than a skip.
fn run_program_with_wasmtime(sources: &[(&str, &str)]) -> Option<std::process::Output> {
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
    let artifact = compile_program_sources(sources).unwrap();
    let id = WASM_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed);
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

fn run_with_wasmtime_args(source: &str, args: &[&str]) -> Option<std::process::Output> {
    run_wasmtime(source, args, None)
}

/// Runs a compiled program under Wasmtime, feeding `input` to the guest's
/// standard input. Used by the stream `read` wrappers.
fn run_with_wasmtime_stdin(source: &str, input: &[u8]) -> Option<std::process::Output> {
    run_wasmtime(source, &[], Some(input))
}

fn run_wasmtime(source: &str, args: &[&str], input: Option<&[u8]>) -> Option<std::process::Output> {
    run_wasmtime_with_dirs(source, args, input, &[])
}

/// Runs a compiled program under Wasmtime with host directories exposed to the
/// guest. Each `(host, guest)` pair becomes a `--dir host::guest` preopen.
fn run_wasmtime_with_dirs(
    source: &str,
    args: &[&str],
    input: Option<&[u8]>,
    dirs: &[(std::path::PathBuf, &str)],
) -> Option<std::process::Output> {
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
    use std::io::Write;
    let artifact = compile_source("Main.purs", source).unwrap();
    let id = WASM_ARTIFACT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("psrs-{}-{id}.wasm", std::process::id()));
    std::fs::write(&path, &artifact.wasm).unwrap();
    let mut command = std::process::Command::new("wasmtime");
    command.arg("run");
    for (host, guest) in dirs {
        command
            .arg("--dir")
            .arg(format!("{}::{}", host.display(), guest));
    }
    command.arg(&path).args(args);
    let output = match input {
        None => command.output().unwrap(),
        Some(input) => {
            command
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let mut child = command.spawn().unwrap();
            child
                .stdin
                .take()
                .expect("stdin is piped")
                .write_all(input)
                .unwrap();
            child.wait_with_output().unwrap()
        }
    };
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

mod adt_template_storage;
mod adts;

mod pattern_matching_audit;

mod source_evidence;

mod arrays;

mod records;

mod functions;

mod module_loader;

mod library_types;

mod tail_calls;
