fn execute(source: &str, stdout: &[u8]) {
    let Some(output) = super::super::run_program_with_prelude_wasmtime(&[("Main.purs", source)])
    else {
        return;
    };
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert_eq!(output.stdout, stdout, "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn pure_bind_delays_its_callback_but_keeps_factory_arguments_strict() {
    execute(
        r#"module Main where
import Prelude
import Effect (Effect)
import Effect.Unsafe (unsafePerformEffect)
import WASI.Console (log)

factory :: Int -> Effect Int
factory n = pure n >>= \value ->
  let _ = unsafePerformEffect (log "callback") in pure (value + 1)

main :: Int
main =
  let argument = let _ = unsafePerformEffect (log "argument") in 41
      action = factory argument
      _ = unsafePerformEffect (log "stored")
      first = unsafePerformEffect action
      second = unsafePerformEffect action
  in first + second - 42
"#,
        b"argument\nstored\ncallback\ncallback\n",
    );
}

#[test]
fn public_loops_monadrec_and_left_and_right_bind_chains_are_stack_safe() {
    execute(
        r#"module Main where
import Prelude
import Control.Monad.Rec.Class (Step(..), tailRecM)
import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect, forE)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)
import WASI.Console (log)

left :: Int -> Effect Int -> Effect Int
left n action = if n == 0 then action else left (n - 1) (action >>= \x -> pure (x + 1))

right :: Int -> Effect Int
right n = if n == 0 then pure 0 else pure unit >>= \_ -> map (_ + 1) (right (n - 1))

mixed :: Effect Int
mixed = do
  ref <- Ref.new 0
  forE 0 20000 \index -> do
    Ref.modify_ (_ + 1) ref
    if index == 0 || index == 19999 then log "edge" else pure unit
  tailRecM (\n -> if n == 20000 then pure (Done unit)
    else do
      Ref.modify_ (_ + 1) ref
      pure (Loop (n + 1))) 0
  Ref.read ref

local :: Int
local = ST.run do
  ref <- STRef.new 0
  ST.for 0 20000 \_ -> void (STRef.modify (_ + 1) ref)
  STRef.read ref

main :: Int
main = if unsafePerformEffect (left 20000 (pure 0)) == 20000
  && unsafePerformEffect (right 20000) == 20000
  && unsafePerformEffect mixed == 40000 && local == 20000
  then 42 else 0
"#,
        b"edge\nedge\n",
    );
}

#[test]
fn repeated_runs_of_one_allocation_action_produce_independent_refs() {
    execute(
        r#"module Main where
import Prelude
import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)

allocate :: Effect (Ref Int)
allocate = Ref.new 0

main :: Int
main = unsafePerformEffect do
  first <- allocate
  second <- allocate
  _ <- Ref.write 42 first
  a <- Ref.read first
  b <- Ref.read second
  if a == 42 && b == 0 then pure 42 else pure 0
"#,
        b"",
    );
}

#[test]
fn global_conversion_and_heterogeneous_cells_preserve_identity_across_runs() {
    execute(
        r#"module Main where
import Prelude
import Control.Monad.ST.Global (toEffect)
import Control.Monad.ST.Ref as STRef
import Effect (Effect)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)

change :: forall a. a -> Ref.Ref a -> Effect a
change value ref = Ref.write value ref *> Ref.read ref

main :: Int
main = unsafePerformEffect do
  global <- toEffect (STRef.new 0)
  _ <- toEffect (STRef.write 40 global)
  before <- toEffect (STRef.read global)
  int <- Ref.new 0
  text <- Ref.new "old"
  function <- Ref.new (\x -> x)
  record <- Ref.new { value: 0, label: "old" }
  let alias = int
  a <- change (before + 1) int
  b <- change "new" text
  c <- change (\x -> x + 1) function
  d <- change { value: 42, label: "new" } record
  seen <- Ref.read alias
  _ <- toEffect (STRef.modify (_ + 2) global)
  after <- toEffect (STRef.read global)
  if a == 41 && seen == 41 && b == "new" && c 41 == 42
    && d.value == 42 && d.label == b && after == 42
    then pure 42 else pure 0
"#,
        b"",
    );
}

#[test]
fn ref_modify_invokes_its_callback_once_and_preserves_reentrant_order() {
    execute(
        r#"module Main where
import Prelude
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)

main :: Int
main = unsafePerformEffect do
  ref <- Ref.new 10
  calls <- Ref.new 0
  result <- Ref.modify' (\old ->
    let _ = unsafePerformEffect (Ref.modify_ (_ + 1) calls)
        _ = unsafePerformEffect (Ref.write 99 ref)
    in { state: old + 1, value: old + 32 }) ref
  now <- Ref.read ref
  count <- Ref.read calls
  writeResult <- Ref.write 12 ref
  last <- Ref.modify (_ + 1) ref
  if result == 42 && now == 11 && count == 1 && writeResult == unit && last == 13
    then pure 42 else pure 0
"#,
        b"",
    );
}

#[test]
fn new_with_self_calls_once_and_publishes_the_same_reference() {
    execute(
        r#"module Main where
import Prelude
import Effect.Ref (Ref)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)

data Node = Node Int (Unit -> Ref Node)

main :: Int
main = unsafePerformEffect do
  count <- Ref.new 0
  ref <- Ref.newWithSelf (\self ->
    let _ = unsafePerformEffect (Ref.modify_ (_ + 1) count)
    in Node 41 (\_ -> self))
  Node before self <- Ref.read ref
  _ <- Ref.write (Node (before + 1) self) (self unit)
  Node after _ <- Ref.read ref
  calls <- Ref.read count
  if before == 41 && after == 42 && calls == 1 then pure 42 else pure 0
"#,
        b"",
    );
}

#[test]
fn an_unpublished_self_read_traps_without_running_the_successor() {
    let source = r#"module Main where
import Prelude
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)
import WASI.Console (log)

main :: Int
main = unsafePerformEffect do
  _ <- Ref.newWithSelf (\self -> unsafePerformEffect (Ref.read self))
  log "unreachable"
  pure 42
"#;
    let Some(output) = super::super::run_program_with_prelude_wasmtime(&[("Main.purs", source)])
    else {
        return;
    };
    assert!(!output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unreachable"),
        "{output:?}"
    );
}
