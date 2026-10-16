//! Official WASI and Test.Assert wrappers after WIT import schemes became live
//! layout roots. Construction stays inert; `unsafePerformEffect` runs `Host.lift`.

const OFFICIAL_HOST_LEAVES: &str = r#"module Main where

import Prelude

import Effect.Unsafe (unsafePerformEffect)
import Test.Assert (assert)
import WASI.Clock (now, wallNow)
import WASI.Console (log)
import WASI.IO (getStdout)
import WASI.Random (randomBytes, randomU64)

main :: Int
main =
  let tick = unsafePerformEffect now * 0
      wall = unsafePerformEffect wallNow
      noise = unsafePerformEffect randomU64 * 0
      bytes = unsafePerformEffect (randomBytes 4)
      _ = unsafePerformEffect (log "host-leaf")
      _ = unsafePerformEffect getStdout
      _ = unsafePerformEffect (assert true)
  in if arrayLength bytes == 4 && wall.seconds * 0 == tick + noise
     then 42
     else 0
"#;

#[test]
fn official_wasi_and_assert_wrappers_run_through_effect() {
    let Some(output) =
        super::super::run_program_with_prelude_wasmtime(&[("Main.purs", OFFICIAL_HOST_LEAVES)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert_eq!(
        output.stdout, b"host-leaf\n",
        "WASI.Console.log must write through Host.lift: {output:?}"
    );
}

const HOST_ORDER: &str = include_str!("fixtures/HostOrder.purs");

#[test]
fn stored_host_work_stays_inert_and_runs_in_source_order() {
    super::assert_matches_stdlib_oracle(HOST_ORDER, "conformance/native/HostOrder.purs");
    let Some(output) =
        super::super::run_program_with_prelude_wasmtime(&[("Main.purs", HOST_ORDER)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert_eq!(output.stdout, b"first\nsecond\n", "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}
