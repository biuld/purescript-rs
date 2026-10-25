//! Catchable initialization demand keeps source origins and never retries work.
use super::*;

const DEMAND: &str = r#"module Main where
import Prelude
import Effect (Effect)
import Effect.Console (log)
import Effect.Exception (try, message)
import Data.Either (Either(..))
applyThunk :: (Unit -> Int) -> Int
applyThunk thunk = thunk unit
bad :: Unit -> Int
bad _ =
  let value = applyThunk
        (\_ ->
          value)
  in value
main :: Effect Unit
main = do
  result <- try (bad <$> pure unit)
  case result of
    Left err -> log (message err)
    Right _ -> log "unexpected success"
"#;

fn execute(sources: &[(&str, &str)], expected: &str) {
    if let Some(output) = run_program_with_prelude_wasmtime(sources) {
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
fn demand_is_caught_at_the_read_line_instead_of_the_binding_line() {
    execute(
        &[("Main.purs", DEMAND)],
        "value was needed before it finished initializing (module Main, line 13)\n",
    );
}

#[test]
fn inlined_helper_keeps_its_binding_module_and_read_line() {
    let helper = DEMAND.split("main ::").next().unwrap().replacen(
        "module Main where",
        "module Helper (bad) where",
        1,
    );
    let main = r#"module Main where
import Prelude
import Helper (bad)
import Effect.Console (log)
import Effect.Exception (try, message)
import Data.Either (Either(..))
main = do
  result <- try (bad <$> pure unit)
  case result of
    Left err -> log (message err)
    Right _ -> log "unexpected success"
"#;
    execute(
        &[
            ("UnrelatedFilename.purs", main),
            ("NotHelper.purs", &helper),
        ],
        "value was needed before it finished initializing (module Helper, line 13)\n",
    );
}

#[test]
fn initialization_demand_requires_exception_capability_even_without_a_catch() {
    let source = r"module Main where
import Prelude
applyThunk :: (Unit -> Int) -> Int
applyThunk thunk = thunk unit
main :: Int
main = let value = applyThunk (\_ -> value) in value
";
    let errors = crate::program::compile_program_sources_with_prelude_target(
        &[("Main.purs", source)],
        psrs_backend::TargetCapabilities {
            exceptions: false,
            ..Default::default()
        },
    )
    .expect_err("compiler-generated raises require native exceptions");
    assert!(
        errors
            .iter()
            .any(|e| e.diagnostic.message.contains("exception-handling proposal")),
        "{errors:?}"
    );
}

const ESCAPED: &str = r#"module Main where
import Prelude
import Effect.Console (log, logShow)
import Effect.Exception (try, message, throw)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)
import Data.Either (Either(..))
remember force left right count a b = unsafePerformEffect do
  Ref.write a left
  Ref.write b right
  Ref.modify_ (_ + 1) count
  if force then pure (a unit) else throw "initializer"
build force left right count =
  let value = remember force left right count
        (\_ -> value)
        (\_ -> value)
  in value
report result = case result of
  Left err -> log (message err)
  Right _ -> log "unexpected success"
main = do
  left <- Ref.new (\_ -> 0)
  right <- Ref.new (\_ -> 0)
  count <- Ref.new 0
  first <- try (build FLAG left right <$> pure count)
  report first
  a <- Ref.read left
  b <- Ref.read right
  again <- try (a <$> pure unit)
  report again
  other <- try (b <$> pure unit)
  report other
  logShow =<< Ref.read count
  independent <- try (build FLAG left right <$> pure count)
  report independent
  logShow =<< Ref.read count
"#;

#[test]
fn escaped_getters_report_each_read_and_never_retry_a_failed_initializer() {
    // Line 15 and line 16 are distinct source reads of the same local value.
    let a = "value was needed before it finished initializing (module Main, line 15)\n";
    let b = "value was needed before it finished initializing (module Main, line 16)\n";
    for (flag, first) in [("true", a), ("false", "initializer\n")] {
        let source = ESCAPED.replace("FLAG", flag);
        execute(
            &[("Main.purs", &source)],
            &format!("{first}{a}{b}1\n{first}2\n"),
        );
    }
}

#[test]
fn initialization_errors_unwind_control_and_allow_fresh_runners() {
    let mut source = DEMAND.split("main ::").next().unwrap().replace("import Prelude\n",
        "import Prelude\nimport PSRS.Control as C\nimport PSRS.Control.Row as R\nimport Effect.Unsafe (unsafePerformEffect)\n");
    source.push_str(
        r#"report result = case result of
  Left err -> log (message err)
  Right _ -> log "unexpected success"
main :: Effect Unit
main = do
  outside <- try ((\_ -> R.runPure (C.bind (C.pure unit) (\_ -> C.pure (bad unit)))) <$> pure unit)
  report outside
  let inside = R.runPure (C.bind (C.pure unit) (\_ -> C.pure (unsafePerformEffect do
        result <- try (bad <$> pure unit)
        report result)))
  pure inside
  log (if R.runPure (C.pure 42) == 42 then "fresh" else "wrong")
"#,
    );
    let error = "value was needed before it finished initializing (module Main, line 16)\n";
    execute(&[("Main.purs", &source)], &format!("{error}{error}fresh\n"));
}
