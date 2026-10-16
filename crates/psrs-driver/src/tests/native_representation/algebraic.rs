const CUSTOM_HANDLER: &str = r#"module Main where
import Prelude
import Effect (Effect)
import Effect.Unsafe (unsafePerformEffect)
import Run (Run)
import Run as Run
import Type.Proxy (Proxy(..))
import WASI.Console (log)

data Query a = Query (Int -> a)
derive instance functorQuery :: Functor Query

query :: Proxy "query"
query = Proxy

program :: Run (query :: Query) Int
program = do
  n <- Run.lift query (Query identity)
  pure (n + 1)

handle :: forall a. Query a -> Effect a
handle (Query k) = do
  log "handler"
  pure (k 41)

main :: Int
main = let action = Run.interpretRec (Run.on query handle Run.case_) program
           _ = unsafePerformEffect (log "stored")
           first = unsafePerformEffect action
           second = unsafePerformEffect action
       in first + second - 42
"#;

#[test]
fn a_custom_algebraic_handler_builds_reusable_native_effects_without_running_early() {
    let Some(output) =
        super::super::run_program_with_prelude_wasmtime(&[("Main.purs", CUSTOM_HANDLER)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert_eq!(output.stdout, b"stored\nhandler\nhandler\n", "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn interpreting_one_operation_does_not_discard_a_residual_row() {
    let source = CUSTOM_HANDLER.replace(
        "program :: Run (query :: Query) Int",
        "program :: Run (query :: Query, residual :: Query) Int",
    );
    let errors = crate::check_program_types_lenient_with_prelude(&[("Main.purs", &source)])
        .expect_err("case_ cannot eliminate the residual operation");
    assert!(
        errors
            .iter()
            .any(|error| error.diagnostic.stage == "P5 typecheck"),
        "{errors:?}"
    );
}
