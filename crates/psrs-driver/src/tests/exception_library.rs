//! EH-16: the library surface — the adapted `Test.Assert.checkThrows` executed
//! end to end.
//!
//! The stdlib's adapted `checkThrows` rides `PSRS.Effect.Native.perform` and
//! the checked `psrs:runtime-exception#checkThrows` binding: the guard runs
//! when the Effect is interpreted, the P9 discharge arms the inline `catch_all`
//! try region around the guarded application, and the budget converts imminent
//! stack exhaustion into the caught exception. The fixtures here compile
//! through the trusted-prelude path (the real frontend publication, not
//! hand-built CC) under the explicitly enabled exception profile, and run
//! under the pinned engine stack limit (EH-15's `-W max-wasm-stack`).

use super::*;
use crate::program::compile_program_sources_with_prelude_target;
use psrs_backend::TargetCapabilities;
use std::path::{Path, PathBuf};

fn enabled_target() -> TargetCapabilities {
    TargetCapabilities {
        exceptions: true,
        ..TargetCapabilities::default()
    }
}

/// The vendored corpus case's inputs, assembled the way the runtime scoreboard
/// assembles a program: the root source plus any same-stem support directory.
fn corpus_case_inputs(stem: &str) -> Vec<(String, String)> {
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream");
    let path = corpus.join("passing").join(format!("{stem}.purs"));
    let text = std::fs::read_to_string(&path).expect("the corpus case is vendored");
    let case = crate::load_program_case_sources(&path, &corpus.join("passing"), &text)
        .expect("the vendored case and its support files remain readable");
    case.own.into_iter().chain(case.loaded).collect()
}

/// The corpus case `TCOMutRec.purs` is rejected under the disabled profile
/// with the channel's own diagnostic (DEC-05's rejection boundary, on the real
/// corpus case rather than a synthetic module).
#[test]
fn tcomutrec_is_rejected_when_the_profile_disables_exceptions() {
    let inputs = corpus_case_inputs("TCOMutRec");
    let sources = inputs
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let disabled = TargetCapabilities {
        exceptions: false,
        ..TargetCapabilities::default()
    };
    let errors = compile_program_sources_with_prelude_target(&sources, disabled)
        .expect_err("the disabled profile rejects the exception-channel call");
    assert!(
        errors.iter().any(|error| error
            .diagnostic
            .message
            .contains("exception-handling proposal")),
        "the diagnostic names the disabled proposal: {errors:?}"
    );
}

/// The full channel through the real frontend: a guard around a genuinely
/// non-tail recursion at the corpus's throwing depth must be interpreted (the
/// guard runs when the Effect runs), exhaust the budget inside the guarded
/// call, catch the compiler's exception, and report `true` — with the
/// returning depth still computing its value first.
#[test]
fn check_throws_observes_the_budget_exception_end_to_end() {
    if super::wasmtime_available().is_none() {
        return;
    }
    let source = r#"module Main where

import Prelude
import Effect (Effect)
import Effect.Console (logShow)
import PSRS.Effect.Native as Target
import Unsafe.Coerce (unsafeCoerce)

foreign import "psrs:runtime-exception#checkThrows" checkThrowsImpl
  :: forall a. (Unit -> a) -> Boolean

checkThrows :: forall a. (Unit -> a) -> Effect Boolean
checkThrows fn = unsafeCoerce (Target.perform \_ -> checkThrowsImpl fn)

-- genuinely non-tail: the caller uses the result in a computation
rec :: Int -> Int -> Int
rec 0 acc = acc
rec n acc = 1 + rec (n - 1) (acc + 1)

main :: Effect Unit
main = do
  logShow (rec 100 0)
  r <- checkThrows \_ -> rec 100000 0
  logShow r
"#;
    let artifact =
        compile_program_sources_with_prelude_target(&[("Main.purs", source)], enabled_target())
            .expect("the guard program must compile under the enabled profile");
    let output = run_artifact_with_wasmtime(&artifact.wasm);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {stdout:?}, stderr: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout, "200\ntrue\n",
        "the returning depth computes, then the throwing depth is caught"
    );
}

/// A guard around an argument that does not throw reports `false`, and the
/// fabricated `Unit` operand is the guarded callable's only argument: the
/// guarded result is discarded, the Boolean comes from the discharge's join.
#[test]
fn check_throws_reports_false_when_the_argument_returns() {
    if super::wasmtime_available().is_none() {
        return;
    }
    let source = r#"module Main where

import Prelude
import Effect (Effect)
import Effect.Console (logShow)
import PSRS.Effect.Native as Target
import Unsafe.Coerce (unsafeCoerce)

foreign import "psrs:runtime-exception#checkThrows" checkThrowsImpl
  :: forall a. (Unit -> a) -> Boolean

checkThrows :: forall a. (Unit -> a) -> Effect Boolean
checkThrows fn = unsafeCoerce (Target.perform \_ -> checkThrowsImpl fn)

ntco1 :: Int -> Int
ntco1 y0 = f 0 y0
  where
  f x = if x > 10 * y0 then (x + _) else g x
    where
    g x' y' = f (x' + 10) (y' - 1)

main :: Effect Unit
main = do
  logShow (ntco1 100)
  r <- checkThrows \_ -> ntco1 100000
  logShow r
"#;
    let artifact =
        compile_program_sources_with_prelude_target(&[("Main.purs", source)], enabled_target())
            .expect("the returning program must compile under the enabled profile");
    let output = run_artifact_with_wasmtime(&artifact.wasm);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {stdout:?}, stderr: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout, "1009\nfalse\n", "no throw, so the guard is false");
}

/// The corpus case `TCOMutRec.purs` runs its depth-100 returning halves under
/// the enabled profile: `ntco1 100` must equal `1009`, the other three
/// depth-100 halves must equal `200`, and the program must log `Done` and exit
/// 0. Cutting the corpus source down to only these assertions keeps the run
/// honest about the normal path: a budget regression reports itself as the
/// depth-100 computation trapping, not as any later throw masking it.
#[test]
fn tcomutrec_depth_100_halves_return_inside_the_budget() {
    if super::wasmtime_available().is_none() {
        return;
    }
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream/passing/TCOMutRec.purs"),
    )
    .expect("TCOMutRec.purs is vendored");
    let head = text
        .split("main :: Effect Unit")
        .next()
        .expect("the corpus source declares main");
    let source = format!(
        "{head}\nmain :: Effect Unit\nmain = do\n  assertEqual {{ expected: 1009, actual: ntco1 100 }}\n  assertEqual {{ expected: 200, actual: ntco2 100 }}\n  assertEqual {{ expected: 200, actual: ntco3 100 }}\n  assertEqual {{ expected: 200, actual: ntco4 100 }}\n  log \"Done\"\n"
    );
    let artifact = compile_program_sources_with_prelude_target(
        &[("Main.purs", source.as_str())],
        enabled_target(),
    )
    .expect("the depth-100 program must compile");
    let output = run_artifact_with_wasmtime(&artifact.wasm);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {stdout:?}, stderr: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout, "Done\n");
}
