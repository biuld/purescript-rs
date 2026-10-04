//! The library's `Functor` class and its `<$>` operator.
//!
//! `Prelude` re-exports the class from `Data.Functor` and adds the `Effect`
//! instance. These tests check that one `map` covers arrays, `Maybe`,
//! `Either`, and `Effect`, and that mapping an effect does not run it.

use super::*;

#[test]
fn functor_map_transforms_array_maybe_and_either() {
    let source = r#"
module Main where

import Prelude
import Data.Either (Either(..))
import Data.Maybe (Maybe(..))
import Effect.Console (log)

main :: Effect Unit
main = do
  log (show ((\n -> n + 1) <$> [1, 2]))
  log (case (\n -> n + 1) <$> Just 1 of
    Just n -> show n
    Nothing -> "nothing")
  log (case (\n -> n + 1) <$> (Right 1 :: Either String Int) of
    Right n -> show n
    Left _ -> "left")
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"[2,3]\n2\n2\n", "{output:?}");
}

#[test]
fn mapping_an_effect_does_not_run_it_until_the_action_runs() {
    let source = r#"
module Main where

import Prelude
import Effect.Console (log)

tick :: Effect Int
tick = do
  log "tick"
  pure 1

main :: Effect Unit
main = do
  let pending = map (\n -> n + 1) tick
  log "before"
  value <- pending
  log (show value)
"#;
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"before\ntick\n2\n", "{output:?}");
}
