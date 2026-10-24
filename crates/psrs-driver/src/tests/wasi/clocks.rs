use super::*;

#[test]
fn exact_and_compatibility_clock_views_share_the_same_wit_import() {
    let source = r#"module Main where
import Prelude
import Effect.Unsafe (unsafePerformEffect)
import WASI.Clock (now, nowWords)
main :: Int
main = unsafePerformEffect do
  coarse <- now
  exact <- nowWords
  later <- now
  -- Low-word subtraction wraps and remains positive for this short interval.
  pure (if exact.low - coarse >= 0 && later - exact.low >= 0 then 42 else 1)
"#;
    let Some(output) = run_with_wasmtime(source) else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}
