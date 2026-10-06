//! `Test.Assert` and the `unit` value it needs.
//!
//! A failing assertion is only observable to whatever runs the program if it
//! reaches the guest as a trap, so these tests assert a trap for the failure
//! path rather than an exit code: the runtime scoreboard counts a non-zero
//! exit as a recorded result, not a failure.

use super::*;

/// Asserts that the guest wrote `message` and then trapped.
///
/// Wasmtime prints its own backtrace after the trap, so the message is matched
/// as the first line rather than as the whole stream: what this asserts is that
/// the library wrote the diagnostic *before* ending the guest.
fn assert_trapped_after_message(output: &std::process::Output, message: &str) {
    assert!(
        !output.status.success(),
        "a failed assertion must trap, not return: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with(message),
        "expected {message:?} first on standard error, got: {stderr}"
    );
}

/// The one `Unit` value, in both spellings the surface language has for it.
const UNIT_SOURCE: &str = r#"
module Main where

import Prelude
import WASI.Console

takesUnit :: Unit -> Int
takesUnit _ = 7

describe :: Int -> String
describe 7 = "unit"
describe _ = "other"

main =
  let
    message = runEffect (log (describe (takesUnit unit)))
    alsoUnit = runEffect (log (describe (takesUnit ())))
  in
    let ignored = message in let ignoredToo = alsoUnit in 0
"#;

#[test]
fn the_unit_value_is_a_core_expression_rather_than_an_integer_literal() {
    // `Unit` has no payload and no constructor table here, so the value is a
    // compiler primitive. Core must name it as itself: lowering it to the
    // integer `0` would make a unit indistinguishable from an `Int` at every
    // later stage.
    let core = lower_source_to_core("Main.purs", UNIT_SOURCE)
        .expect("typechecking and lowering `unit` into Core");
    let dump = format!("{core:#?}");
    assert!(
        dump.contains("Unit"),
        "Core must carry the unit value as its own expression: {dump}"
    );
}

#[test]
fn the_unit_value_executes_when_wasmtime_is_available() {
    let Some(output) = run_with_wasmtime(UNIT_SOURCE) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        output.stdout, b"unit\nunit\n",
        "both `unit` and `()` must reach the same value: {output:?}"
    );
}

#[test]
fn a_held_assertion_lets_the_program_finish_when_wasmtime_is_available() {
    let source = "module Main where\nimport Prelude\nimport Test.Assert\nheld :: Effect Unit\nheld = do\n  assert true\n  assertTrue true\n  assertFalse false\n  assert' \"unused\" true\n  pure unit\nmain = let ignored = runEffect held in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"", "a held assertion writes nothing");
}

#[test]
fn a_failed_assertion_traps_with_its_message_when_wasmtime_is_available() {
    let source = "module Main where\nimport Prelude\nimport Test.Assert\nmain = let ignored = runEffect (assert false) in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_trapped_after_message(&output, "Assertion failed\n");
}

#[test]
fn a_failed_assertion_writes_the_message_before_it_traps() {
    // `assert'` carries the diagnostic, and the standard error write must
    // happen before the trap: the trap ends the guest, so a later write would
    // never reach the host.
    let source = "module Main where\nimport Prelude\nimport Test.Assert\nmain = let ignored = runEffect (assert' \"Expected 42\" false) in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_trapped_after_message(&output, "Expected 42\n");
}

#[test]
fn a_statement_after_a_failed_assertion_never_runs() {
    // The escape is the end of the effect chain, not a value that evaluation
    // continues past, so the statement after the trap never gets to write.
    let source = "module Main where\nimport Prelude\nimport Test.Assert\nimport WASI.Console\nfailed :: Effect Unit\nfailed = do\n  assert false\n  _ <- log \"unreachable\"\n  pure unit\nmain = let ignored = runEffect failed in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"", "nothing runs after the trap");
}

#[test]
fn assert_true_and_assert_false_report_the_value_that_did_not_hold() {
    let source = "module Main where\nimport Prelude\nimport Test.Assert\nmain = let ignored = runEffect (assertTrue false) in 0\n";
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    // The message names the value that did not hold, the way the official
    // module's `assertTrue` does: `assertEqual'` renders `Expected`/`Actual`
    // without an extra prefix.
    assert_trapped_after_message(&output, "Expected: true\nActual:   false\n");
}

/// The renderings `Data.Show` produces, so this fails if `logShow` grows a
/// stringifier of its own instead of composing the library `show`.
///
/// The negative case is written `0 - 7` rather than `- 7` on purpose: unary
/// minus resolves through an ordinary in-scope `negate`, which this library
/// does not declare yet, and this test is about the `Show` rendering.
#[test]
fn log_show_writes_the_library_rendering() {
    let source = r#"
module Main where

import Prelude
import Effect.Console (logShow)

checks :: Effect Unit
checks = do
  logShow 42
  logShow (0 - 7)
  logShow "hi"
  logShow 'c'
  logShow true
  logShow [1, 2, 3]
  pure unit

main = let ignored = runEffect checks in 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        output.stdout, b"42\n-7\n\"hi\"\n'c'\ntrue\n[1,2,3]\n",
        "`logShow` must write exactly what `Data.Show.show` produces: {output:?}"
    );
}
