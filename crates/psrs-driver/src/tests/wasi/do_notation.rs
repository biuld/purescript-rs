use super::*;

#[test]
fn runs_a_do_notation_effect_sequence_when_wasmtime_is_available() {
    // `do` desugars to `bind`/`discard`/`let`. The block runs the two writes
    // left to right, binds a value for the final `pure`, and `runEffect` yields
    // the entry's exit code.
    let source = r#"module Main where
import Prelude
import WASI.Console

main = runEffect
  do
    log "first"
    let label = "second"
    log label
    value <- pure 40
    pure (value + 2)
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert_eq!(output.stdout, b"first\nsecond\n");
}
