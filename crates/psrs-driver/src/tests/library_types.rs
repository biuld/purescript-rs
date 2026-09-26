use super::*;

#[test]
fn runs_library_maybe_and_either_by_casing_on_just_and_left() {
    let source = "\
module Main where
import Data.Maybe
import Data.Either

justValue :: Maybe Int -> Int
justValue value = case value of
  Just inner -> inner
  Nothing -> 0

leftValue :: Either Int Int -> Int
leftValue value = case value of
  Left inner -> inner
  Right other -> other

main = justValue (Just (leftValue (Left (fromMaybe 0 (hush (note 0 (Just (maybe 0 (\\n -> n + 0) (Just (either (\\n -> n + 0) (\\n -> n) (Right 42)))))))))))
";
    let artifact = compile_source("Main.purs", source).expect("library Maybe and Either");
    assert!(artifact.wasm.len() > 8);
    let Some(output) = run_with_wasmtime(source) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42));
}
