//! Value-sensitive execution of persistent continuations through closed `run`.

mod control_support;
use control_support::execute;

fn source(program: &str, handler: &str, result: &str) -> String {
    format!(
        r#"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> {{ _1 :: Int, _2 :: C.Control Op Int }}
handler s (Op k) cont = {handler}
program :: C.Control Op Int
program = {program}
result :: {{ _1 :: Int, _2 :: Int }}
result = C.run 0 handler program
main :: Int
main = {result}
"#
    )
}

const NEXT: &str = "{ _1: intAdd s 1, _2: cont (k s) }";

#[test]
fn bind_associations_preserve_order_and_state() {
    for (name, program) in [
        (
            "left",
            r"C.bind (C.bind (C.perform (Op \x -> x)) (\a -> C.return (intAdd a 40))) (\b -> C.bind (C.perform (Op \x -> x)) (\c -> C.return (intAdd b c)))",
        ),
        (
            "right",
            r"C.bind (C.perform (Op \x -> x)) (\a -> C.bind (C.return (intAdd a 40)) (\b -> C.bind (C.perform (Op \x -> x)) (\c -> C.return (intAdd b c))))",
        ),
    ] {
        execute(
            name,
            &source(
                program,
                NEXT,
                "if intEq result._1 2 then intAdd result._2 1 else 1",
            ),
            42,
        );
    }
}

#[test]
fn a_handler_can_abandon_the_pending_sequence() {
    execute(
        "abandon",
        &source(
            r"C.bind (C.perform (Op \x -> x)) (\_ -> C.return (intQuot 1 0))",
            "{ _1: 7, _2: C.return 42 }",
            "if intEq result._1 7 then result._2 else 1",
        ),
        42,
    );
}

#[test]
fn a_handler_can_replay_then_bind_a_saved_continuation() {
    execute(
        "replay",
        &source(
            r"C.bind (C.perform (Op \x -> x)) (\a -> C.bind (C.return (intAdd a 1)) (\b -> C.return b))",
            r"{ _1: intAdd s 1, _2: C.bind (cont (k 19)) (\a -> C.bind (cont (k 21)) (\b -> C.return (intAdd a b))) }",
            "if intEq result._1 1 then result._2 else 1",
        ),
        42,
    );
}

#[test]
fn replayed_requests_share_current_handler_state_without_consuming_saved_frames() {
    execute(
        "replayed-requests",
        &source(
            r"C.bind (C.perform (Op \x -> x)) (\a -> C.bind (C.perform (Op \x -> x)) (\b -> C.return (intAdd a b)))",
            r"if intEq s 0 then { _1: 1, _2: C.bind (cont (k 19)) (\a -> C.bind (cont (k 20)) (\b -> C.return (intAdd a b))) } else { _1: intAdd s 1, _2: cont (k s) }",
            "if intEq result._1 3 then result._2 else 1",
        ),
        42,
    );
}

#[test]
fn nested_runs_have_independent_pending_sequences() {
    execute(
        "nested",
        &source(
            r"C.bind (C.perform (Op \x -> x)) (\a -> let inner = C.run 40 handler (C.bind (C.perform (Op \x -> x)) (\b -> C.return (intAdd b 1))) in C.return (intAdd a inner._2))",
            NEXT,
            "if intEq result._1 1 then intAdd result._2 1 else 1",
        ),
        42,
    );
}

#[test]
fn constructing_a_computation_does_not_interpret_requests() {
    execute(
        "inert",
        &source(
            r"let unused = C.bind (C.perform (Op \x -> x)) (\_ -> C.return (intQuot 1 0)) in C.return 42",
            "{ _1: intQuot 1 0, _2: cont (k s) }",
            "if intEq result._1 0 then result._2 else 1",
        ),
        42,
    );
}

#[test]
fn ignored_run_results_preserve_handler_traps() {
    let text = source(
        r"C.perform (Op \x -> x)",
        "{ _1: intQuot 1 0, _2: cont (k s) }",
        "let ignored = result in 42",
    );
    // Wasmtime maps a guest trap to exit 134 on this runner.
    execute("ignored-trap", &text, 134);
}

#[test]
fn computation_factory_arguments_are_strict() {
    execute(
        "strict-factory",
        &source(
            "let unused = C.return (intQuot 1 0) in C.return 42",
            NEXT,
            "result._2",
        ),
        134,
    );
}

#[test]
fn repeating_interpretation_keeps_computation_nodes_immutable() {
    execute(
        "repeat-run",
        &source(
            r"C.bind (C.perform (Op \x -> x)) (\a -> C.return (intAdd a 1))",
            NEXT,
            "let other = C.run 40 handler program in if intEq result._1 1 then if intEq other._1 41 then intAdd result._2 other._2 else 1 else 1",
        ),
        42,
    );
}

#[test]
fn long_bind_chains_and_request_sequences_are_stack_safe() {
    for (name, definition) in [
        (
            "long-left",
            r"build n c = if intEq n 0 then c else build (intSub n 1) (C.bind c (\a -> C.return (intAdd a 1)))",
        ),
        (
            "long-right",
            r"build n c = if intEq n 0 then c else C.bind c (\a -> build (intSub n 1) (C.return (intAdd a 1)))",
        ),
        (
            "long-requests",
            r"build n c = if intEq n 0 then c else C.bind (C.perform (Op \x -> x)) (\_ -> build (intSub n 1) (C.bind c (\a -> C.return (intAdd a 1))))",
        ),
    ] {
        let mut text = source(
            "build 100000 (C.return 0)",
            NEXT,
            "if intEq result._2 100000 then 42 else 1",
        );
        text.push_str("\nbuild :: Int -> C.Control Op Int -> C.Control Op Int\n");
        text.push_str(definition);
        text.push('\n');
        execute(name, &text, 42);
    }
}

#[test]
fn erased_aggregate_answers_state_and_captures_are_recovered() {
    execute(
        "aggregate",
        r#"module Main where
import Prim.Control as C
data Op next = Op ({ value :: Int } -> next)
handler :: forall x. { count :: Int } -> Op x -> (x -> C.Control Op { answer :: Int }) -> { _1 :: { count :: Int }, _2 :: C.Control Op { answer :: Int } }
handler s (Op k) cont = { _1: { count: intAdd s.count 1 }, _2: cont (k { value: 40 }) }
program :: C.Control Op { answer :: Int }
program = let saved = { offset: 2 } in C.bind (C.perform (Op \x -> x)) (\a -> C.return { answer: intAdd a.value saved.offset })
result :: { _1 :: { count :: Int }, _2 :: { answer :: Int } }
result = C.run { count: 0 } handler program
main :: Int
main = if intEq result._1.count 1 then result._2.answer else 1
"#,
        42,
    );
}

#[test]
fn collection_preserves_array_and_string_captures_across_suspension() {
    execute_gc(
        "buffer-gc",
        r#"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
data Noise next = Noise (Int -> next)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler s (Op k) cont = let churn = C.run 0 noiseHandler (noise 4000) in { _1: intAdd s churn._1, _2: cont (k 0) }
noiseHandler :: forall x. Int -> Noise x -> (x -> C.Control Noise Int) -> { _1 :: Int, _2 :: C.Control Noise Int }
noiseHandler s (Noise k) cont = { _1: intAdd s 1, _2: cont (k s) }
noise :: Int -> C.Control Noise Int
noise n = if intEq n 0 then C.return 0 else C.bind (C.perform (Noise \x -> x)) (\_ -> noise (intSub n 1))
program :: C.Control Op Int
program = let numbers = [40, 2]
              text = stringToBytes (numberToString 10.0)
          in C.bind (C.perform (Op \x -> x)) (\_ -> C.return (if intEq (arrayLength text) 0 then 0 else intAdd (arrayIndex numbers 0) (arrayLength numbers)))
main :: Int
main = (C.run 0 handler program)._2
"#,
    );
}

fn execute_gc(name: &str, source: &str) {
    control_support::execute_with_gc(name, source, 42);
}

#[test]
fn function_answers_use_the_shared_callable_protocol() {
    execute(
        "function",
        r#"module Main where
import Prim.Control as C
data Op next = Op ((Int -> Int) -> next)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler s (Op k) cont = { _1: s, _2: cont (k (\n -> intAdd n 2)) }
program :: C.Control Op Int
program = C.bind (C.perform (Op \x -> x)) (\f -> C.return (f 40))
main :: Int
main = (C.run 0 handler program)._2
"#,
        42,
    );
}
