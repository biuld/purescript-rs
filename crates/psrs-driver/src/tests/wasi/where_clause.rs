use super::*;

#[test]
fn runs_a_program_with_where_clauses_when_wasmtime_is_available() {
    // A value-declaration `where` and a case-alternative `where` each lower to
    // a local `let`, and their bindings are in scope for the right-hand side.
    // The effect block logs and then returns `describe 39 + fromJust (Just 0) 0
    // = 42` as the entry's exit code.
    let source = r#"module Main where
import Prelude
import WASI.Console

data Maybe a = Nothing | Just a

describe :: Int -> Int
describe n = adjusted where
  adjusted = n + bonus
  bonus = 2

fromJust :: Maybe Int -> Int -> Int
fromJust m fallback = case m of
  Just value -> result where
    result = value + 1
  Nothing -> fallback

main = runEffect
  do
    log "where clauses"
    pure (describe 39 + fromJust (Just 0) 0)
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert_eq!(output.stdout, b"where clauses\n");
}
