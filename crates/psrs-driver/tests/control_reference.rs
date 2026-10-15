//! Pinned independent-package Control/Run fixtures from
//! `psrs-stdlib/conformance/control/`. They execute ordinary library wrappers.
mod control_support;

#[test]
fn locked_run_state_threads_put_modify_and_get() {
    control_support::execute(
        "run-state",
        r"module Main where
import Prelude
import Run (Run, extract)
import Run.State (State, put, modify, get, evalState)
program :: Run (state :: State Int) Int
program = do
  put 20
  modify (\value -> value + 22)
  get
main :: Int
main = extract (evalState 0 program)
",
        42,
    );
}

#[test]
fn locked_run_choose_executes_both_alternatives() {
    control_support::execute(
        "run-choose",
        r"module Main where
import Prelude
import Run (Run, extract)
import Run.Choose (Choose, calt, runChoose)
import Data.Array (length)
program :: Run (choose :: Choose) Int
program = calt (pure 20) (pure 22)
answers :: Array Int
answers = extract (runChoose program)
main :: Int
main = if answers == [20, 22] && length answers == 2 then 42 else 0
",
        42,
    );
}

#[test]
fn locked_run_handlers_compose_reader_state_writer_and_except() {
    control_support::execute(
        "run-handlers",
        r#"module Main where
import Prelude
import Data.Either (Either(..))
import Data.Tuple (Tuple(..))
import Run (Run, extract)
import Run.Reader (Reader, ask, local, runReader)
import Run.State (State, put, modify, get, evalState)
import Run.Writer (Writer, tell, censor, foldWriter)
import Run.Except (Except, throw, runExcept)
program :: Run (reader :: Reader Int, state :: State Int, writer :: Writer String, except :: Except String) Int
program = do
  seed <- ask
  put seed
  local (\n -> n + 1) do
    adjusted <- ask
    modify (\n -> n + adjusted)
  tell "a"
  censor (\s -> s <> "!") (tell "b")
  value <- get
  if value == 41 then pure (value + 1) else throw "incorrect state"
main :: Int
main = case extract (runExcept (foldWriter (\a b -> a <> b) "" (evalState 0 (runReader 20 program)))) of
  Right (Tuple log answer) -> if log == "ab!" then answer else 0
  Left _ -> 0
"#,
        42,
    );
}

#[test]
fn locked_labelled_rows_deep_forwards_and_reinstalls() {
    control_support::execute(
        "rows-deep",
        r#"module Main where
import Prelude
import PSRS.Control as C
import PSRS.Control.Row as R
import Type.Proxy (Proxy(..))
data Read x = Read (Int -> x)
derive instance functorRead :: Functor Read
program :: R.Computation (left :: Read, right :: Read) Int
program = C.bind (R.performAt (Proxy :: Proxy "left") (Read identity)) \left ->
  C.bind (R.performAt (Proxy :: Proxy "right") (Read identity)) \right ->
    C.bind (R.performAt (Proxy :: Proxy "left") (Read identity)) (\_ -> C.pure (left + right))
handleLeft :: forall x. Int -> Read x -> (Int -> x -> R.Computation (right :: Read) { _1 :: Int, _2 :: Int }) -> R.Computation (right :: Read) { _1 :: Int, _2 :: Int }
handleLeft state (Read answer) resume = resume (state + 1) (answer 20)
handleRight :: forall x. Int -> Read x -> (Int -> x -> R.Computation () { _1 :: Int, _2 :: { _1 :: Int, _2 :: Int } }) -> R.Computation () { _1 :: Int, _2 :: { _1 :: Int, _2 :: Int } }
handleRight state (Read answer) resume = resume (state + 1) (answer 22)
main :: Int
main = let result = R.runPure (R.handleDeepAt (Proxy :: Proxy "right") handleRight 0 (R.handleDeepAt (Proxy :: Proxy "left") handleLeft 0 program))
  in if result._1 == 1 && result._2._1 == 2 then result._2._2 else 0
"#,
        42,
    );
}

#[test]
fn locked_labelled_rows_shallow_escape_keeps_the_residual() {
    control_support::execute(
        "rows-shallow",
        r#"module Main where
import Prelude
import PSRS.Control as C
import PSRS.Control.Row as R
import Type.Proxy (Proxy(..))
data Read x = Read (Int -> x)
derive instance functorRead :: Functor Read
program :: R.Computation (ask :: Read) Int
program = C.bind (R.performAt (Proxy :: Proxy "ask") (Read identity)) \first ->
  C.bind (R.performAt (Proxy :: Proxy "ask") (Read identity)) \second -> C.pure (first + second)
inner :: forall x. Int -> Read x -> (Int -> x -> R.Computation (ask :: Read) { _1 :: Int, _2 :: Int }) -> R.Computation (ask :: Read) { _1 :: Int, _2 :: Int }
inner state (Read answer) resume = resume (state + 1) (answer 20)
outer :: forall x. Int -> Read x -> (Int -> x -> R.Computation () { _1 :: Int, _2 :: { _1 :: Int, _2 :: Int } }) -> R.Computation () { _1 :: Int, _2 :: { _1 :: Int, _2 :: Int } }
outer state (Read answer) resume = resume (state + 1) (answer 22)
main :: Int
main = let result = R.runPure (R.handleDeepAt (Proxy :: Proxy "ask") outer 0 (R.handleShallowAt (Proxy :: Proxy "ask") inner 0 program))
  in if result._1 == 1 && result._2._1 == 1 then result._2._2 else 0
"#,
        42,
    );
}

#[test]
fn locked_labelled_rows_reject_a_nonempty_residual_at_run_pure() {
    let source = r"module Main where
import PSRS.Control as C
import PSRS.Control.Row as R
data Read x = Read x
program :: R.Computation (ask :: Read) Int
program = C.pure 42
main :: Int
main = R.runPure program
";
    let errors = psrs_driver::check_source("Main.purs", source).expect_err("residual row");
    assert!(
        errors
            .iter()
            .any(|error| error.code == Some("TypesDoNotUnify")),
        "{errors:?}"
    );
}
