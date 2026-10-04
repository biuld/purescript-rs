//! The library's `Show` class.
//!
//! These tests execute under Wasmtime. The strings are the official `Show`
//! spellings: `true`/`false`, decimal `Int`, `unit`, quoted `Char` and
//! `String` (with the official escapes), arrays without spaces, and `Number`
//! with a trailing `.0` on an integer token.

use super::*;

#[test]
fn show_renders_the_instances_the_corpus_prints() {
    let source = r#"
module Main where

import Prelude
import Effect.Console (log)

checks :: Effect Unit
checks = do
  log (show true)
  log (show false)
  log (show 0)
  log (show 42)
  log (show (0 - 7))
  log (show ((0 - 2147483647) - 1))
  log (show unit)
  log (show "hi")
  log (show "a\"b")
  log (show "line\n")
  log (show 'a')
  log (show '\n')
  log (show '\'')
  log (show [1, 2, 3])
  log (show [1.0, 2.0])
  log (show 0.0)
  log (show 1.0)
  log (show (numberNeg 2.0))
  log (show 1.5)
  log (show 0.5)
  log (show 0.25)
  log (show 10000000000.0)
  log (show 1.0e21)
  log (show 1.0e-5)
  log (show "é")
  log (show "cafés")
  pure unit

main = let ignored = runEffect checks in 0
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected = concat!(
        "true\n",
        "false\n",
        "0\n",
        "42\n",
        "-7\n",
        "-2147483648\n",
        "unit\n",
        "\"hi\"\n",
        "\"a\\\"b\"\n",
        "\"line\\n\"\n",
        "'a'\n",
        "'\\n'\n",
        "'\\''\n",
        "[1,2,3]\n",
        "[1.0,2.0]\n",
        "0.0\n",
        "1.0\n",
        "-2.0\n",
        "1.5\n",
        "0.5\n",
        "0.25\n",
        "10000000000.0\n",
        "1e+21\n",
        "0.00001\n",
        "\"é\"\n",
        "\"cafés\"\n",
    );
    assert_eq!(
        stdout.as_ref(),
        expected,
        "stdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_type_without_show_is_rejected() {
    let source = r#"
module Main where

import Prelude

data Box = Box

main :: String
main = show Box
"#;
    let errors = check_program_types_lenient_with_prelude(&[("Main.purs", source)])
        .expect_err("Box is not a Show");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.code == Some("NoInstanceFound")),
        "expected NoInstanceFound, got {errors:?}"
    );
}
