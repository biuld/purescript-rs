use super::*;

#[test]
fn wit_result_bytes_recover_values_and_empty_arrays_when_wasmtime_is_available() {
    let source = r#"module Main where
import Prelude
import Data.Either (Either(..))
import WASI.IO (getStdin, blockingRead)
main =
  let stream = runEffect getStdin in
  let empty = runEffect (blockingRead stream 0) in
  case empty of
    Left _ -> 1
    Right zero ->
      let result = runEffect (blockingRead stream 4) in
      case result of
        Left _ -> 2
        Right bytes ->
          if arrayLength zero == 0 && arrayLength bytes == 4
              && arrayIndex bytes 0 == 0 && arrayIndex bytes 1 == 255
              && arrayIndex bytes 2 == 128 && arrayIndex bytes 3 == 42
          then 42
          else 3
"#;
    let Some(output) = run_with_wasmtime_stdin(source, &[0, 255, 128, 42, 99]) else {
        eprintln!("skipping: wasmtime is not installed");
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}
