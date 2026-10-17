#[test]
fn the_configured_native_command_runner_executes_effect_unit_once() {
    let source = r#"module Main where
import Prelude
import Effect (Effect)
import WASI.Console (log)
main :: Effect Unit
main = do
  log "command"
  pure unit
"#;
    let Some(output) = super::super::run_program_with_prelude_wasmtime(&[("Main.purs", source)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"command\n", "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn the_native_command_runner_propagates_traps_from_non_unit_actions() {
    let source = r#"module Main where
import Prelude
import Effect (Effect)
import Test.Assert (assert)
import WASI.Console (log)
main :: Effect Int
main = do
  log "before"
  assert false
  log "after"
  pure 42
"#;
    let Some(output) = super::super::run_program_with_prelude_wasmtime(&[("Main.purs", source)])
    else {
        return;
    };
    assert!(!output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"before\n", "{output:?}");
}

#[test]
fn the_native_command_runner_discards_a_non_unit_result_after_execution() {
    let source = r#"module Main where
import Prelude
import Effect (Effect)
import WASI.Console (log)
main :: Effect Int
main = do
  log "result"
  pure 42
"#;
    let Some(output) = super::super::run_program_with_prelude_wasmtime(&[("Main.purs", source)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, b"result\n", "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn the_native_command_runner_rejects_an_unrelated_carrier() {
    let invalid = "module Main where\nmain :: Array Int\nmain = [42]\n";
    let errors = crate::compile_program_sources_with_prelude(&[("Main.purs", invalid)])
        .expect_err("the ordinary runner accepts only its declared input");
    assert!(
        errors.iter().any(|error| error
            .diagnostic
            .message
            .contains("command entry type disagrees with the configured runner input")),
        "{errors:?}"
    );
}
