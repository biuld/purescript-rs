//! The L6/M7 runtime scoreboard.
//!
//! # What agreement means here
//!
//! The corpus has no execution goldens: `tests/upstream` vendors `.purs` sources
//! only, and upstream's own `passing` suite is a compile-time suite, so its
//! ground truth is "this module type checks". Our criterion is therefore
//! stronger and narrower at once: a `passing` file **agrees** when the pipeline
//! compiles it, the generated component passes Wasm validation, and the guest
//! runs to completion under Wasmtime without trapping. Nothing in the corpus
//! requires argv, stdin, or a preopened directory, so the runner passes none;
//! no corpus case needs them yet, and `PSRS_SUITE_LIMIT` and
//! `PSRS_SUITE_FILTER` narrow a run for a quick iteration.
//!
//! The observable result is recorded, not compared: `main`'s return value is the
//! process exit code and its stdout is captured, so `main = 42` agrees with exit
//! code 42. A trap or a hang is a failure. A `Test.Assert` failure must reach the
//! guest as a trap, which this board counts as a failure, and that is the only
//! execution signal the corpus can express without goldens.
//!
//! Every rejection is reported per file with its first blocking stage and
//! diagnostic, and the two categories a later phase recovers are counted
//! separately: a missing module is Phase 3's to land, while a rejection in
//! lowering is Phase 2's.

use super::corpus::{
    collected_files, corpus_root, is_ffi_excluded, suite_filter, suite_limit, support_sources,
};
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// How long one case may run before it counts as a hang. A corpus program is a
/// pure computation, so a case that has not finished by now is not going to.
const RUN_TIMEOUT: Duration = Duration::from_secs(10);

const RUN_POLL: Duration = Duration::from_millis(10);

/// Wasmtime prints this for any guest trap, whatever the cause.
const TRAP_MARKERS: [&str; 2] = ["wasm trap", "wasm backtrace"];

/// How the guest terminated, after it was given the component to run.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RunOutcome {
    /// The guest ran to completion. The exit code is `main`'s return value.
    Completed {
        exit_code: Option<i32>,
        stdout: String,
    },
    /// The guest trapped, or the process died from a signal.
    Trapped(String),
    /// The guest was still running at the deadline.
    TimedOut,
}

/// Decides the run outcome from what the process left behind. Pure, so the
/// three shapes are unit-tested without a trapping program.
fn classify_run(timed_out: bool, exit_code: Option<i32>, stdout: &str, stderr: &str) -> RunOutcome {
    if timed_out {
        return RunOutcome::TimedOut;
    }
    let trap = TRAP_MARKERS
        .iter()
        .any(|marker| stderr.contains(marker))
        .then(|| first_line(stderr).to_owned())
        .or_else(|| {
            exit_code
                .is_none()
                .then(|| "terminated by a signal, not by returning".to_owned())
        });
    match trap {
        Some(detail) => RunOutcome::Trapped(detail),
        None => RunOutcome::Completed {
            exit_code,
            stdout: stdout.to_owned(),
        },
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("").trim()
}

/// Compiles, validates, and runs every non-FFI `passing` file.
///
/// Compilation includes Wasm validation: the backend rejects a component that
/// does not validate before returning it, so a compiled artifact has already
/// been validated.
#[test]
#[ignore = "requires wasmtime and the vendored corpus"]
fn l6_runtime_scoreboard() {
    let Some(repo) = corpus_root() else {
        eprintln!("skipping: vendored corpus not found and PURESCRIPT_REPO is unset");
        return;
    };
    let Some(()) = wasmtime_usable() else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    check_the_runner();

    let limit = suite_limit();
    let filter = suite_filter();
    let mut considered = 0usize;
    let mut excluded = 0usize;
    let mut agree = 0usize;
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut stages: BTreeMap<String, usize> = BTreeMap::new();
    let mut messages: BTreeMap<String, usize> = BTreeMap::new();
    let mut missing_module = 0usize;
    let mut exit_codes: BTreeMap<String, usize> = BTreeMap::new();
    let mut samples: Vec<(String, String)> = Vec::new();

    for path in collected_files(&repo.join("passing"), limit) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative_path = path
            .strip_prefix(&repo)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        if let Some(filter) = filter.as_deref()
            && !relative_path.contains(filter)
        {
            continue;
        }
        if is_ffi_excluded(&path, &text) {
            excluded += 1;
            continue;
        }
        considered += 1;
        let sources = support_sources(&path, &text);
        let inputs: Vec<(&str, &str)> = sources
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let outcome = match psrs_driver::compile_program_sources_with_prelude(&inputs) {
            Err(errors) => {
                let stage = errors
                    .first()
                    .map_or("unknown", |error| error.diagnostic.stage)
                    .to_owned();
                *stages.entry(stage.clone()).or_default() += 1;
                let first = errors
                    .first()
                    .map_or("no diagnostic", |error| error.diagnostic.message.as_str());
                *messages.entry(first.to_owned()).or_default() += 1;
                if errors
                    .iter()
                    .any(|error| error.diagnostic.code == Some("ModuleNotFound"))
                {
                    missing_module += 1;
                }
                let code = errors
                    .first()
                    .and_then(|error| error.diagnostic.code)
                    .unwrap_or("");
                failures.push((relative_path, format!("{stage} [{code}]: {first}")));
                continue;
            }
            Ok(artifact) => run_component(&artifact.wasm),
        };
        match outcome {
            RunOutcome::Completed { exit_code, stdout } => {
                agree += 1;
                let code = match exit_code {
                    Some(code) => code.to_string(),
                    None => "signal".to_owned(),
                };
                *exit_codes.entry(code).or_default() += 1;
                samples.push((relative_path, describe(&stdout)));
            }
            RunOutcome::Trapped(detail) => {
                failures.push((relative_path, format!("trapped: {detail}")));
            }
            RunOutcome::TimedOut => {
                failures.push((
                    relative_path,
                    format!("did not finish within {}s", RUN_TIMEOUT.as_secs()),
                ));
            }
        }
    }

    println!("runtime agreement: {agree}/{considered}, {excluded} excluded");
    println!("blocked on a missing module: {missing_module}");
    println!("compilation failures by stage:");
    for (stage, count) in &stages {
        println!("  {stage}: {count}");
    }
    println!("most common rejection messages:");
    let mut ranked: Vec<_> = messages.iter().collect();
    ranked.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    for (message, count) in ranked.iter().take(15) {
        println!("  {count} x {message}");
    }
    if agree > 0 {
        println!("exit codes:");
        for (code, count) in &exit_codes {
            println!("  {code}: {count}");
        }
    }
    println!("failing cases ({}):", failures.len());
    for (path, reason) in &failures {
        println!("  {path}: {reason}");
    }
    if agree > 0 {
        println!("observable results of the {agree} agreeing cases:");
        for (path, observed) in &samples {
            println!("  {path}: {observed}");
        }
    }
}

/// Exercises the whole board path once before it measures anything: compile,
/// validate, run, classify. A board that reports zero because its runner is
/// broken is worse than no board, so this panics instead of printing a number.
fn check_the_runner() {
    let artifact = psrs_driver::compile_program_sources_with_prelude(&[(
        "Main.purs",
        "module Main where\nmain = 42\n",
    )])
    .expect("the runner self-check must compile");
    assert_eq!(
        run_component(&artifact.wasm),
        RunOutcome::Completed {
            exit_code: Some(42),
            stdout: String::new(),
        },
        "the runner self-check must execute a compiled component and read back `main`"
    );
}

/// Runs one component under Wasmtime with no argv, no stdin, and no preopens.
fn run_component(wasm: &[u8]) -> RunOutcome {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let stem = std::env::temp_dir().join(format!("psrs-l6-{}-{id}", std::process::id()));
    let wasm_path = stem.with_extension("wasm");
    let stdout_path = stem.with_extension("out");
    let stderr_path = stem.with_extension("err");
    let write = |path: &std::path::Path, bytes: &[u8]| std::fs::write(path, bytes);
    write(&wasm_path, wasm).expect("writing the component to run");
    let stdout = std::fs::File::create(&stdout_path).expect("creating the stdout capture");
    let stderr = std::fs::File::create(&stderr_path).expect("creating the stderr capture");
    let child = Command::new("wasmtime")
        .arg("run")
        .arg(&wasm_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn();
    let outcome = match child {
        Err(error) => RunOutcome::Trapped(format!("wasmtime could not start: {error}")),
        Ok(mut child) => {
            let deadline = Instant::now() + RUN_TIMEOUT;
            let (timed_out, status) = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break (false, Ok(status)),
                    Err(error) => break (false, Err(error)),
                    Ok(None) if Instant::now() >= deadline => {
                        let _ = child.kill();
                        break (true, child.wait());
                    }
                    Ok(None) => std::thread::sleep(RUN_POLL),
                }
            };
            let code = status.as_ref().ok().and_then(|status| status.code());
            let read = |path: &std::path::Path| std::fs::read_to_string(path).unwrap_or_default();
            classify_run(timed_out, code, &read(&stdout_path), &read(&stderr_path))
        }
    };
    for path in [&wasm_path, &stdout_path, &stderr_path] {
        let _ = std::fs::remove_file(path);
    }
    outcome
}

/// Reports what a program printed, or that it printed nothing.
fn describe(stdout: &str) -> String {
    match first_line(stdout) {
        "" => "no output".to_owned(),
        line => format!("stdout: {line}"),
    }
}

/// Returns `Some(())` when Wasmtime can be run. A missing or unusable toolchain
/// is a skip, unless `PSRS_REQUIRE_WASMTIME=1` makes it a failure, so this
/// board cannot silently pass a machine that cannot execute a component.
fn wasmtime_usable() -> Option<()> {
    let version = Command::new("wasmtime").arg("--version").output();
    let required = std::env::var("PSRS_REQUIRE_WASMTIME").as_deref() == Ok("1");
    match &version {
        Ok(output) if output.status.success() => Some(()),
        other => {
            if required {
                panic!("PSRS_REQUIRE_WASMTIME=1 but wasmtime is not usable: {other:?}");
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_exit_with_no_stderr_is_a_completed_run() {
        assert_eq!(
            classify_run(false, Some(0), "", ""),
            RunOutcome::Completed {
                exit_code: Some(0),
                stdout: String::new(),
            }
        );
    }

    #[test]
    fn the_main_return_value_is_the_exit_code_not_a_failure() {
        assert_eq!(
            classify_run(false, Some(42), "", ""),
            RunOutcome::Completed {
                exit_code: Some(42),
                stdout: String::new(),
            }
        );
    }

    #[test]
    fn printed_output_is_captured_without_changing_the_verdict() {
        assert_eq!(
            classify_run(false, Some(0), "Hello λ→ world!!\n", ""),
            RunOutcome::Completed {
                exit_code: Some(0),
                stdout: "Hello λ→ world!!\n".to_owned(),
            }
        );
    }

    #[test]
    fn a_wasm_trap_on_stderr_is_a_trap_not_a_non_zero_exit() {
        let stderr = "Error: failed to run main module `x.wasm`\n\nCaused by:\n    0: error while executing at wasm backtrace: main\n    1: wasm trap: wasm `unreachable` instruction executed\n";
        assert_eq!(
            classify_run(false, Some(1), "", stderr),
            RunOutcome::Trapped("Error: failed to run main module `x.wasm`".to_owned())
        );
    }

    #[test]
    fn a_signal_termination_is_a_trap() {
        assert_eq!(
            classify_run(false, None, "", ""),
            RunOutcome::Trapped("terminated by a signal, not by returning".to_owned())
        );
    }

    #[test]
    fn the_deadline_wins_over_any_output() {
        assert_eq!(
            classify_run(
                true,
                None,
                "partial",
                "wasm trap: wasm `unreachable` instruction executed"
            ),
            RunOutcome::TimedOut
        );
    }
}
