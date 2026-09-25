//! Control-flow and tail-call execution evidence (CF-01..CF-13).
//!
//! Self tail recursion is loopified on every profile, so a deep self-recursive
//! program runs in constant stack. Non-self tail calls become `return_call` /
//! `return_call_ref` only when the target enables the tail-call proposal; the
//! stable profile keeps them as ordinary calls plus return.

use super::*;
use psrs_backend::TargetCapabilities;
use psrs_backend::mir::Terminator;

fn required_wasmtime() -> bool {
    std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1")
}

fn execute_component(name: &str, wasm: &[u8]) -> Result<Option<std::process::Output>, String> {
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
        std::env::temp_dir().join(format!("psrs-tail-call-{}-{id}.wasm", std::process::id()));
    std::fs::write(&path, wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg("run")
        .arg(&path)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);
    Ok(Some(output))
}

fn compile_with(source: &str, target: TargetCapabilities) -> psrs_backend::Stages {
    let core = lower_source_to_core("Main.purs", source).expect("source should lower to Core");
    psrs_backend::compile_with_target(core, target).expect("the program should compile")
}

fn has_tail_call(mir: &psrs_backend::mir::Module) -> bool {
    mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| {
            matches!(
                &block.terminator,
                Some(Terminator::ReturnCall { .. } | Terminator::ReturnCallRef { .. })
            )
        })
    })
}

fn has_return_call_ref(mir: &psrs_backend::mir::Module) -> bool {
    mir.functions.iter().any(|function| {
        function
            .blocks
            .iter()
            .any(|block| matches!(&block.terminator, Some(Terminator::ReturnCallRef { .. })))
    })
}

#[test]
fn self_tail_recursion_runs_in_constant_stack() {
    // 100000 frames would exhaust the default Wasmtime stack; loopification
    // must make this a `loop` on the tail-call-disabled stable profile.
    let source = "module Main where\ncount :: Int -> Int\ncount n = if n == 0 then 42 else count (n - 1)\nmain = count 100000\n";
    let stages = compile_with(source, TargetCapabilities::default());
    assert!(
        !has_tail_call(&stages.mir),
        "self-recursion must become a loop, not a return_call"
    );
    match execute_component("self_tail_loop", &stages.artifact.wasm) {
        Ok(Some(output)) => assert_eq!(output.status.code(), Some(42), "{output:?}"),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn non_self_tail_calls_use_return_call_when_enabled() {
    let source = "module Main where\neven :: Int -> Int\neven n = if n == 0 then 42 else odd (n - 1)\nodd :: Int -> Int\nodd n = if n == 0 then 7 else even (n - 1)\nmain = even 100000\n";
    let target = TargetCapabilities {
        tail_call: true,
        ..TargetCapabilities::default()
    };
    let stages = compile_with(source, target);
    assert!(
        has_tail_call(&stages.mir),
        "mutual tail recursion must lower to return_call on an enabled profile"
    );
    assert!(
        stages.artifact.wat.contains("return_call"),
        "the encoded module must contain a return_call opcode"
    );
    match execute_component("non_self_tail_call", &stages.artifact.wasm) {
        Ok(Some(output)) => assert_eq!(output.status.code(), Some(42), "{output:?}"),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn indirect_tail_recursion_uses_return_call_ref() {
    // `run` tail-calls its function argument; `tick` tail-calls `run` directly.
    // With 100000 iterations this only succeeds if the reference tail call
    // reuses the frame.
    let source = "module Main where\nrun :: (Int -> Int) -> Int -> Int\nrun step n = if n == 0 then 42 else step (n - 1)\ntick :: Int -> Int\ntick n = run tick n\nmain = tick 100000\n";
    let target = TargetCapabilities {
        tail_call: true,
        ..TargetCapabilities::default()
    };
    let stages = compile_with(source, target);
    assert!(
        has_return_call_ref(&stages.mir),
        "an indirect tail call must lower to return_call_ref"
    );
    assert!(
        stages.artifact.wat.contains("return_call_ref"),
        "the encoded module must contain a return_call_ref opcode"
    );
    match execute_component("indirect_tail_call", &stages.artifact.wasm) {
        Ok(Some(output)) => assert_eq!(output.status.code(), Some(42), "{output:?}"),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn disabled_profile_keeps_an_ordinary_call() {
    let source = "module Main where\nf :: Int -> Int\nf x = g x\ng :: Int -> Int\ng x = x + 1\nmain = f 41\n";
    let stages = compile_with(source, TargetCapabilities::default());
    assert!(
        !has_tail_call(&stages.mir),
        "a disabled profile must not emit a tail-call terminator"
    );
    assert!(
        !stages.artifact.wat.contains("return_call"),
        "a disabled profile must not encode a return_call opcode"
    );
    match execute_component("ordinary_call", &stages.artifact.wasm) {
        Ok(Some(output)) => assert_eq!(output.status.code(), Some(42), "{output:?}"),
        Ok(None) => {}
        Err(error) => panic!("{error}"),
    }
}
