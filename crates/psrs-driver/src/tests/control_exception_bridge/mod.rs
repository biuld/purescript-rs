//! Source-level composition through the real frontend and optimized backend.
use super::*;

mod lifecycle;

const HEADER: &str = r#"module Main where
import Prelude
import Prim.Control as C
foreign import "psrs:runtime-exception#checkThrows" caught :: forall a. (Unit -> a) -> Boolean
data Op x = Op (Int -> x)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler s (Op k) raw = { _1: s, _2: raw (k 0) }
rec :: Int -> Int
rec n = if intEq n 0 then 0 else intAdd 1 (rec (intSub n 1))
"#;

fn execute(body: &str) -> std::process::Output {
    let source = format!("{}\n{body}", HEADER);
    let artifact = crate::program::compile_program_sources_with_prelude_target(
        &[("Main.purs", source.as_str())],
        psrs_backend::TargetCapabilities::default(),
    )
    .expect("bridge source compiles through the real frontend");
    run_artifact_with_wasmtime(&artifact.wasm)
}

#[test]
fn callback_and_owned_callback_exhaustion_propagate_and_allow_a_fresh_run() {
    if wasmtime_available().is_none() {
        return;
    }
    for computation in [
        r"C.bind (C.perform (Op (\x -> x))) (\_ -> C.return (rec 100000))",
        r"C.resume (C.once (\n -> C.return (rec n))) 100000",
        r"C.handle C.return (\(Op k) raw -> raw (k 0)) (C.bind (C.perform (Op (\x -> x))) (\_ -> C.return (rec 100000)))",
    ] {
        let source = format!(
            r"
main :: Int
main = if caught (\_ -> (C.run 0 handler ({computation}))._2)
       then (C.run 0 handler (C.return 42))._2 else 0
"
        );
        let output = execute(&source);
        assert_eq!(output.status.code(), Some(42), "{computation}: {output:?}");
    }
}

#[test]
fn caught_callback_exception_does_not_abort_the_runner() {
    if wasmtime_available().is_none() {
        return;
    }
    let output = execute(
        r"
main :: Int
main = (C.run 0 handler
  (C.bind (C.perform (Op (\x -> x)))
    (\_ -> if caught (\_ -> rec 100000) then C.return 42 else C.return 0)))._2
",
    );
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn repeated_capture_cancel_abandon_and_throw_do_not_leak_native_budget() {
    if wasmtime_available().is_none() {
        return;
    }
    let output = execute(
        r"
abandon :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
abandon s (Op k) raw =
  let owner = C.once (\n -> raw (k n))
      discarded = C.cancel owner
  in { _1: s, _2: C.return 0 }
repeat :: Int -> Int -> Int
repeat n total = if intEq n 0 then total else
  let ignored = (C.run 0 abandon (C.perform (Op (\x -> x))))._2
      thrown = caught (\_ -> (C.run ignored handler (C.resume (C.once (\x -> C.return (rec x))) 100000))._2)
  in if thrown then repeat (intSub n 1) (intAdd total 1) else 0
main :: Int
main = if intEq (repeat 40 0) 40 then if intEq (rec 100) 100 then 42 else 0 else 0
",
    );
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn escaped_raw_continuation_expires_after_exceptional_runner_exit() {
    if wasmtime_available().is_none() {
        return;
    }
    let output = execute(
        r#"
foreign import "psrs:intrinsic#arrayFill" filled :: forall a. Int -> a -> Array a
foreign import "psrs:intrinsic#arrayWrite" slot :: forall a. Array a -> Int -> a -> Array a
foreign import "psrs:intrinsic#arrayIndex" at :: forall a. Array a -> Int -> a
foreign import "psrs:intrinsic#arrayLength" size :: forall a. Array a -> Int
capture :: Array (Int -> C.Control Op Int) -> forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
capture saved s (Op k) raw =
  { _1: s
  , _2: C.bind (C.return (size (slot saved 0 (\n -> raw (k n))))) (\_ -> C.return (rec 100000))
  }
main :: Int
main = let saved = filled 1 (\n -> C.return n)
       in if caught (\_ -> (C.run 0 (capture saved) (C.perform (Op (\x -> x))))._2)
          then (C.run 0 handler ((at saved 0) 42))._2 else 0
"#,
    );
    assert!(
        !output.status.success() && output.status.code() != Some(42),
        "expired checkpoint must trap: {output:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unreachable"),
        "{output:?}"
    );
}

#[test]
fn bridge_runners_reject_a_profile_without_native_exceptions() {
    let source = format!("{}\nmain = (C.run 0 handler (C.return 42))._2\n", HEADER);
    let errors = crate::program::compile_program_sources_with_prelude_target(
        &[("Main.purs", source.as_str())],
        psrs_backend::TargetCapabilities {
            exceptions: false,
            ..Default::default()
        },
    )
    .expect_err("required cleanup cannot be silently removed");
    assert!(
        errors
            .iter()
            .any(|e| e.diagnostic.message.contains("exception-handling")),
        "{errors:?}"
    );
}

#[test]
fn nested_run_abort_can_be_caught_without_expiring_the_outer_checkpoint() {
    if wasmtime_available().is_none() {
        return;
    }
    let output = execute(
        r"
outer :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
outer s (Op k) raw =
  { _1: s
  , _2: if caught (\_ -> (C.run 0 handler (C.resume (C.once (\n -> C.return (rec n))) 100000))._2)
        then raw (k 42) else C.return 0
  }
main :: Int
main = (C.run 0 outer (C.perform (Op (\x -> x))))._2
",
    );
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn native_catch_during_resume_construction_has_expired_at_interpretation() {
    if wasmtime_available().is_none() {
        return;
    }
    let output = execute(
        r"
build :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
build s (Op k) raw =
  let next = raw (k 0)
  in { _1: s, _2: if caught (\_ -> next) then C.return 0 else next }
main :: Int
main = if caught (\_ -> (C.run 0 build
  (C.bind (C.perform (Op (\x -> x))) (\_ -> C.return (rec 100000))))._2)
  then 42 else 0
",
    );
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn exceptional_cleanup_cannot_be_removed_reordered_or_fabricate_an_answer() {
    use psrs_backend::cc::{AssignmentKind as A, ControlHelperRole as R, RegionExit};
    fn change(body: &mut [psrs_backend::cc::Assignment], mutation: usize) -> bool {
        for assignment in body {
            match &mut assignment.kind {
                A::Protect {
                    handler, exception, ..
                } => {
                    match mutation {
                        0 => handler.assignments.clear(),
                        1 => {
                            handler.assignments.reverse();
                        }
                        2 => handler.exit = RegionExit::Yield(*exception),
                        _ => handler.exit = RegionExit::Propagate(assignment.destination),
                    }
                    return true;
                }
                A::TagSwitch { cases, .. } => {
                    for case in cases {
                        if change(&mut case.assignments, mutation) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }
    let source = include_str!("../../../tests/control_support/nominal_carriers.purs");
    for role in [R::Run, R::Executor] {
        for mutation in 0..4 {
            let mut input = crate::lower_main_to_cc(source).unwrap();
            let symbol = input
                .cc
                .representations
                .control_helpers()
                .iter()
                .find(|helper| helper.role() == role)
                .unwrap()
                .function();
            let function = input
                .cc
                .functions
                .iter_mut()
                .find(|function| function.symbol == symbol)
                .unwrap();
            assert!(change(&mut function.assignments, mutation));
            assert!(
                psrs_backend::cc::verify_module(&input.cc).is_err(),
                "{role:?}/{mutation}"
            );
        }
    }
}

#[test]
fn replay_in_an_inner_runner_preserves_the_live_outer_saved_checkpoint() {
    if wasmtime_available().is_none() {
        return;
    }
    let output = execute(
        r"
replay :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
replay s (Op k) raw =
  { _1: s
  , _2: if intEq ((C.run 0 handler (raw (k 20)))._2) 20 then raw (k 42) else C.return 0
  }
main :: Int
main = (C.run 0 replay (C.bind (C.perform (Op (\x -> x))) C.return))._2
",
    );
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn saved_queues_and_cancellation_leave_the_same_native_exhaustion_depth() {
    if wasmtime_available().is_none() {
        return;
    }
    let output = execute(
        r#"
foreign import "psrs:intrinsic#arrayFill" filled :: forall a. Int -> a -> Array a
foreign import "psrs:intrinsic#arrayWrite" slot :: forall a. Array a -> Int -> a -> Array a
foreign import "psrs:intrinsic#arrayIndex" at :: forall a. Array a -> Int -> a
foreign import "psrs:intrinsic#arrayLength" size :: forall a. Array a -> Int
counted :: Array Int -> Int -> Int
counted cell n = if intEq n 0 then 0 else
  intAdd (size (slot cell 0 (intAdd (at cell 0) 1))) (counted cell (intSub n 1))
measure :: Array Int -> Int
measure cell = let reset = slot cell 0 0
               in if caught (\_ -> counted reset 100000) then at cell 0 else 0
discard :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
discard s (Op k) raw =
  let owner = C.once (\n -> raw (k n))
      cancelled = C.cancel owner
  in { _1: s, _2: C.return 0 }
main :: Int
main = let cell = filled 1 0
           before = measure cell
           ignored = (C.run 0 discard (C.perform (Op (\x -> x))))._2
           after = intAdd ignored (measure cell)
       in if intEq before after then if intLt 0 before then 42 else 0 else 0
"#,
    );
    assert_eq!(output.status.code(), Some(42), "{output:?}");
}

#[test]
fn repeated_owned_resumes_keep_the_executor_loop_outside_callback_protection() {
    if wasmtime_available().is_none() {
        return;
    }
    let output = execute(
        r"
steps :: Int -> C.Control Op Int
steps n = if intEq n 0 then C.return 42 else
  C.bind (C.resume (C.once (\x -> C.return x)) n) (\_ -> steps (intSub n 1))
main :: Int
main = (C.run 0 handler (steps 100000))._2
",
    );
    assert_eq!(
        output.status.code(),
        Some(42),
        "owner protection must end before the next executor step: {output:?}"
    );
}
