//! Contract v2: source/target scope, persistent replay and extent completion.
mod control_support;
use control_support::execute;

const HEADER: &str = r"module Main where
import Prim.Control as C
data Op next = Op Int (Int -> next)
handleDeep :: forall u f g s a. (forall x r. u x -> (f x -> r) -> (g x -> r) -> r) -> (forall x. s -> f x -> (s -> x -> C.Control g { _1 :: s, _2 :: a }) -> C.Control g { _1 :: s, _2 :: a }) -> s -> C.Control u a -> C.Control g { _1 :: s, _2 :: a }
handleDeep project clause state computation = C.handle
  (\value -> C.return { _1: state, _2: value })
  (\request raw -> project request
    (\op -> clause state op (\nextState answer -> handleDeep project clause nextState (raw answer)))
    (\op -> C.bind (C.perform op) (\answer -> handleDeep project clause state (raw answer))))
  computation
handleShallow :: forall u f g s a. (forall x r. u x -> (f x -> r) -> (g x -> r) -> r) -> (forall x. s -> f x -> (s -> x -> C.Control u { _1 :: s, _2 :: a }) -> C.Control u { _1 :: s, _2 :: a }) -> s -> C.Control u a -> C.Control u { _1 :: s, _2 :: a }
handleShallow project clause state computation = C.handle
  (\value -> C.return { _1: state, _2: value })
  (\request raw -> project request
    (\op -> clause state op (\nextState answer -> C.bind (raw answer) (\value -> C.return { _1: nextState, _2: value })))
    (\_ -> C.bind (C.perform request) (\answer -> handleShallow project clause state (raw answer))))
  computation
project :: forall x r. Op x -> (Op x -> r) -> (Op x -> r) -> r
project op selected residual = selected op
outer :: forall x. Int -> Op x -> (x -> C.Control Op { _1 :: Int, _2 :: Int }) -> { _1 :: Int, _2 :: C.Control Op { _1 :: Int, _2 :: Int } }
outer s (Op label k) cont = { _1: intAdd s 1, _2: cont (k 21) }
";

fn check(name: &str, program: &str, expected: i32) {
    execute(name, &format!("{HEADER}\n{program}"), expected);
}

#[test]
fn fresh_clause_requests_reach_the_outer_handler() {
    check(
        "scoped-clause",
        r"
inner :: forall x. Int -> Op x -> (Int -> x -> C.Control Op { _1 :: Int, _2 :: Int }) -> C.Control Op { _1 :: Int, _2 :: Int }
inner s (Op label k) cont = C.bind (C.perform (Op 2 (\x -> x))) (\n -> cont (intAdd s 1) (k (intAdd n 21)))
result = C.run 0 outer (handleDeep project inner 0 (C.perform (Op 1 (\x -> x))))
main = if intEq result._1 1 then if intEq result._2._1 1 then result._2._2 else 0 else 0
",
        42,
    );
}

#[test]
fn deep_and_shallow_have_distinct_source_resumption_scopes() {
    let body = r"
inner :: forall x. Int -> Op x -> (Int -> x -> C.Control Op { _1 :: Int, _2 :: Int }) -> C.Control Op { _1 :: Int, _2 :: Int }
inner s (Op label k) cont = cont (intAdd s 1) (k 21)
program = C.bind (C.perform (Op 1 (\x -> x))) (\a -> C.bind (C.perform (Op 1 (\x -> x))) (\b -> C.return (intAdd a b)))
result = C.run 0 outer (HANDLE project inner 0 program)
main = if intEq result._1 OUTER then if intEq result._2._1 INNER then result._2._2 else 0 else 0
";
    check(
        "scoped-deep",
        &body
            .replace("HANDLE", "handleDeep")
            .replace("OUTER", "0")
            .replace("INNER", "2"),
        42,
    );
    check(
        "scoped-shallow",
        &body
            .replace("HANDLE", "handleShallow")
            .replace("OUTER", "1")
            .replace("INNER", "1"),
        42,
    );
}

#[test]
fn return_clause_effects_execute_outside_source_dispatch() {
    check(
        "scoped-return",
        r"
ret :: Int -> C.Control Op { _1 :: Int, _2 :: Int }
ret a = C.bind (C.perform (Op 2 (\x -> x))) (\b -> C.return { _1: 7, _2: intAdd a b })
clause :: forall x. Op x -> (x -> C.Control Op Int) -> C.Control Op { _1 :: Int, _2 :: Int }
clause request raw = C.return { _1: 0, _2: intQuot 1 0 }
result = C.run 0 outer (C.handle ret clause (C.return 21))
main = if intEq result._1 1 then result._2._2 else 0
",
        42,
    );
}

#[test]
fn deep_replay_keeps_independent_state_and_caller_work_outside_capture() {
    check(
        "scoped-replay",
        r"
inner :: forall x. Int -> Op x -> (Int -> x -> C.Control Op { _1 :: Int, _2 :: Int }) -> C.Control Op { _1 :: Int, _2 :: Int }
inner s (Op label k) cont = C.bind (cont 3 (k 19)) (\a -> C.bind (cont 7 (k 21)) (\b -> C.return { _1: intAdd a._1 b._1, _2: intAdd a._2 b._2 }))
program = C.bind (C.perform (Op 1 (\x -> x))) (\a -> C.return (intAdd a 1))
result = C.run 0 outer (C.bind (handleDeep project inner 0 program) (\pair -> C.bind (C.perform (Op 2 (\x -> x))) (\_ -> C.return pair)))
main = if intEq result._1 1 then if intEq result._2._1 10 then result._2._2 else 0 else 0
",
        42,
    );
}

#[test]
fn abandonment_never_executes_the_captured_source_queue() {
    check(
        "scoped-abandon",
        r"
inner :: forall x. Int -> Op x -> (Int -> x -> C.Control Op { _1 :: Int, _2 :: Int }) -> C.Control Op { _1 :: Int, _2 :: Int }
inner s op cont = C.return { _1: 7, _2: 42 }
program = C.bind (C.perform (Op 1 (\x -> x))) (\_ -> C.return (intQuot 1 0))
result = C.run 0 outer (handleDeep project inner 0 program)
main = if intEq result._1 0 then if intEq result._2._1 7 then result._2._2 else 0 else 0
",
        42,
    );
}

const ESCAPE: &str = r"
type Saved = { raw :: Int -> C.Control Op Int, built :: C.Control Op Int }
ret :: Int -> C.Control Op Saved
ret a = C.return { raw: \x -> C.return x, built: C.return a }
clause :: forall x. Op x -> (x -> C.Control Op Int) -> C.Control Op Saved
clause (Op label k) raw = C.return { raw: \n -> raw (k n), built: raw (k 42) }
root :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
root s (Op label k) raw = { _1: s, _2: raw (k 42) }
handled = C.handle ret clause (C.perform (Op 1 (\x -> x)))
main = (C.run 0 root (C.bind handled (\saved -> USE)))._2
";

#[test]
fn target_completion_expires_raw_continuations_while_parent_is_live() {
    check(
        "scoped-expired-call",
        &ESCAPE.replace("USE", "saved.raw 42"),
        134,
    );
    check(
        "scoped-expired-node",
        &ESCAPE.replace("USE", "saved.built"),
        134,
    );
}

#[test]
fn handle_alias_and_partial_application_keep_distinct_answer_types() {
    check(
        "scoped-alias",
        r"
eliminate :: forall u g a b. (a -> C.Control g b) -> (forall x. u x -> (x -> C.Control u a) -> C.Control g b) -> C.Control u a -> C.Control g b
eliminate = C.handle
ret :: Int -> C.Control Op Boolean
ret n = C.return (intEq n 42)
clause :: forall x. Op x -> (x -> C.Control Op Int) -> C.Control Op Boolean
clause op raw = C.return false
finish :: C.Control Op Int -> C.Control Op Boolean
finish = eliminate ret clause
root :: forall x. Int -> Op x -> (x -> C.Control Op Boolean) -> { _1 :: Int, _2 :: C.Control Op Boolean }
root s (Op label k) raw = { _1: s, _2: raw (k 0) }
main = if (C.run 0 root (finish (C.return 42)))._2 then 42 else 0
",
        42,
    );
}

#[test]
fn long_deep_sequences_do_not_grow_the_native_stack() {
    check(
        "scoped-long",
        r"
inner :: forall x. Int -> Op x -> (Int -> x -> C.Control Op { _1 :: Int, _2 :: Int }) -> C.Control Op { _1 :: Int, _2 :: Int }
inner s (Op label k) cont = cont (intAdd s 1) (k 0)
build :: Int -> C.Control Op Int
build n = if intEq n 0 then C.return 42 else C.bind (C.perform (Op 1 (\x -> x))) (\_ -> build (intSub n 1))
result = C.run 0 outer (handleDeep project inner 0 (build 100000))
main = if intEq result._2._1 100000 then result._2._2 else 0
",
        42,
    );
}
