//! Value-sensitive execution of open elimination and persistent forwarding.

mod control_support;
use control_support::execute;

fn source(program: &str, inner: &str, outer: &str, result: &str) -> String {
    format!(
        r#"module Main where
import Prim.Control as C
data F next = F (Int -> next)
data G next = G (Int -> next)
data U next = UF (F next) | UG (G next)
project :: forall x r. U x -> (F x -> r) -> (G x -> r) -> r
project u onF onG = case u of
  UF f -> onF f
  UG g -> onG g
inject :: forall x. G x -> U x
inject = UG
inner :: forall x. Int -> F x -> (x -> C.Control U Int) -> {{ _1 :: Int, _2 :: C.Control U Int }}
inner s (F k) cont = {inner}
outer :: forall x. Int -> G x -> (x -> C.Control G {{ _1 :: Int, _2 :: Int }}) -> {{ _1 :: Int, _2 :: C.Control G {{ _1 :: Int, _2 :: Int }} }}
outer s (G k) cont = {outer}
program :: C.Control U Int
program = {program}
handled :: C.Control G {{ _1 :: Int, _2 :: Int }}
handled = foldOpen 0 inner project inject program
result :: {{ _1 :: Int, _2 :: {{ _1 :: Int, _2 :: Int }} }}
result = C.run 0 outer handled
main :: Int
main = {result}
"#
    )
}

const INNER: &str = "{ _1: intAdd s 1, _2: cont (k s) }";
const OUTER: &str = "{ _1: intAdd s 1, _2: cont (k 40) }";
const MIXED: &str = r"C.bind (C.perform (UF (F \x -> x))) (\a -> C.bind (C.perform (UG (G \x -> x))) (\b -> C.bind (C.perform (UF (F \x -> x))) (\c -> C.return (intAdd a (intAdd b c)))))";

#[test]
fn handles_and_forwards_then_reinstalls_the_inner_delimiter() {
    execute(
        "open-mixed",
        &source(
            MIXED,
            INNER,
            OUTER,
            "if intEq result._1 1 then if intEq result._2._1 2 then intAdd result._2._2 1 else 1 else 1",
        ),
        42,
    );
}

#[test]
fn constructing_an_open_handler_is_inert() {
    let text = source(MIXED, "{ _1: intQuot 1 0, _2: cont (k s) }", OUTER, "42")
        .replace("main = 42", "main = let unused = handled in 42");
    execute("open-inert", &text, 42);
}

#[test]
fn open_handler_can_abandon_its_remainder() {
    execute(
        "open-abandon",
        &source(
            r"C.bind (C.perform (UF (F \x -> x))) (\_ -> C.return (intQuot 1 0))",
            "{ _1: 7, _2: C.return 42 }",
            OUTER,
            "if intEq result._1 0 then if intEq result._2._1 7 then result._2._2 else 1 else 1",
        ),
        42,
    );
}

#[test]
fn residual_handler_can_abandon_the_inner_delimiter() {
    execute(
        "residual-abandon",
        &source(
            r"C.bind (C.perform (UG (G \x -> x))) (\_ -> C.return (intQuot 1 0))",
            INNER,
            "{ _1: 7, _2: C.return { _1: 0, _2: 42 } }",
            "if intEq result._1 7 then result._2._2 else 1",
        ),
        42,
    );
}

#[test]
fn residual_replay_preserves_each_branchs_inner_state() {
    execute(
        "residual-replay",
        &source(
            MIXED,
            INNER,
            r"{ _1: intAdd s 1, _2: C.bind (cont (k 19)) (\a -> C.bind (cont (k 20)) (\b -> C.return { _1: intAdd a._1 b._1, _2: intAdd a._2 b._2 })) }",
            "if intEq result._1 1 then if intEq result._2._1 4 then intAdd result._2._2 1 else 1 else 1",
        ),
        42,
    );
}

#[test]
fn open_handler_can_replay_its_saved_sequence() {
    execute(
        "open-replay",
        &source(
            r"C.bind (C.perform (UF (F \x -> x))) (\a -> C.return (intAdd a 1))",
            r"{ _1: intAdd s 1, _2: C.bind (cont (k 19)) (\a -> C.bind (cont (k 21)) (\b -> C.return (intAdd a b))) }",
            OUTER,
            "if intEq result._2._1 1 then result._2._2 else 1",
        ),
        42,
    );
}

#[test]
fn forwarded_remainders_finish_before_the_outer_bind() {
    let text = source(MIXED, INNER, OUTER, "result._2._2").replace(
        "C.run 0 outer handled",
        "C.run 0 outer (C.bind handled (\\a -> C.return { _1: a._1, _2: intAdd a._2 1 }))",
    );
    execute("forward-order", &text, 42);
}

#[test]
fn clause_requests_are_interpreted_with_updated_inner_state() {
    execute(
        "open-clause-request",
        &source(
            r"C.perform (UF (F \x -> x))",
            r"if intEq s 0 then { _1: 1, _2: C.bind (C.perform (UG (G \x -> x))) (\a -> cont (k (intAdd a 2))) } else { _1: s, _2: cont (k s) }",
            OUTER,
            "if intEq result._2._1 1 then result._2._2 else 1",
        ),
        42,
    );
}

#[test]
fn long_forwarded_request_sequences_are_stack_safe() {
    let mut text = source(
        "build 100000",
        INNER,
        OUTER,
        "if intEq result._1 100000 then if intEq result._2._1 100000 then result._2._2 else 1 else 1",
    );
    text.push_str(r"
build :: Int -> C.Control U Int
build n = if intEq n 0 then C.return 42 else C.bind (C.perform (UF (F \x -> x))) (\_ -> C.bind (C.perform (UG (G \x -> x))) (\_ -> build (intSub n 1)))");
    execute("long-forward", &text, 42);
}

#[test]
fn repeated_interpretation_preserves_open_nodes() {
    execute(
        "open-repeat",
        &source(
            MIXED,
            INNER,
            OUTER,
            "let other = C.run 0 outer handled in intAdd result._2._2 (intSub other._2._2 40)",
        ),
        42,
    );
}

#[test]
fn nested_open_handlers_remove_distinct_effects_in_order() {
    execute(
        "open-nested",
        r#"module Main where
import Prim.Control as C
data F x = F (Int -> x)
data G x = G (Int -> x)
data H x = H (Int -> x)
data U x = UF (F x) | UG (G x) | UH (H x)
data V x = VG (G x) | VH (H x)
projectF :: forall x r. U x -> (F x -> r) -> (V x -> r) -> r
projectF u l r = case u of
  UF f -> l f
  UG g -> r (VG g)
  UH h -> r (VH h)
injectF :: forall x. V x -> U x
injectF v = case v of
  VG g -> UG g
  VH h -> UH h
projectG :: forall x r. V x -> (G x -> r) -> (H x -> r) -> r
projectG v l r = case v of
  VG g -> l g
  VH h -> r h
handlerF :: forall x. Int -> F x -> (x -> C.Control U Int) -> { _1 :: Int, _2 :: C.Control U Int }
handlerF s (F k) cont = { _1: intAdd s 1, _2: cont (k s) }
handlerG :: forall x. Int -> G x -> (x -> C.Control V { _1 :: Int, _2 :: Int }) -> { _1 :: Int, _2 :: C.Control V { _1 :: Int, _2 :: Int } }
handlerG s (G k) cont = { _1: intAdd s 1, _2: cont (k s) }
handlerH :: forall x. Int -> H x -> (x -> C.Control H { _1 :: Int, _2 :: { _1 :: Int, _2 :: Int } }) -> { _1 :: Int, _2 :: C.Control H { _1 :: Int, _2 :: { _1 :: Int, _2 :: Int } } }
handlerH s (H k) cont = { _1: intAdd s 1, _2: cont (k s) }
program :: C.Control U Int
program = C.bind (C.perform (UF (F \x -> x))) (\a -> C.bind (C.perform (UG (G \x -> x))) (\b -> C.bind (C.perform (UH (H \x -> x))) (\c -> C.bind (C.perform (UF (F \x -> x))) (\d -> C.return (intAdd a (intAdd b (intAdd c d)))))))
result = C.run 30 handlerH (foldOpen 10 handlerG projectG VH (foldOpen 0 handlerF projectF injectF program))
main :: Int
main = if intEq result._1 31 then if intEq result._2._1 11 then if intEq result._2._2._1 2 then intAdd result._2._2._2 1 else 1 else 1 else 1
"#,
        42,
    );
}

#[test]
fn open_handler_recovers_aggregate_answers_state_and_captures() {
    execute(
        "open-aggregate",
        r#"module Main where
import Prim.Control as C
data F x = F ({ value :: Int } -> x)
data G x = G ({ value :: Int } -> x)
data U x = UF (F x) | UG (G x)
project :: forall x r. U x -> (F x -> r) -> (G x -> r) -> r
project u l r = case u of
  UF f -> l f
  UG g -> r g
inner :: forall x. { count :: Int } -> F x -> (x -> C.Control U { answer :: Int }) -> { _1 :: { count :: Int }, _2 :: C.Control U { answer :: Int } }
inner s (F k) cont = { _1: { count: intAdd s.count 1 }, _2: cont (k { value: 1 }) }
outer :: forall x. Int -> G x -> (x -> C.Control G { _1 :: { count :: Int }, _2 :: { answer :: Int } }) -> { _1 :: Int, _2 :: C.Control G { _1 :: { count :: Int }, _2 :: { answer :: Int } } }
outer s (G k) cont = { _1: intAdd s 1, _2: cont (k { value: 39 }) }
program :: C.Control U { answer :: Int }
program = let captured = { offset: 2 } in C.bind (C.perform (UG (G \x -> x))) (\a -> C.bind (C.perform (UF (F \x -> x))) (\b -> C.return { answer: intAdd captured.offset (intAdd a.value b.value) }))
result = C.run 0 outer (foldOpen { count: 0 } inner project UG program)
main :: Int
main = if intEq result._1 1 then if intEq result._2._1.count 1 then result._2._2.answer else 1 else 1
"#,
        42,
    );
}

#[test]
fn ignored_open_interpretation_retains_handler_traps() {
    execute(
        "open-ignored-trap",
        &source(
            MIXED,
            "{ _1: intQuot 1 0, _2: cont (k s) }",
            OUTER,
            "let ignored = result in 42",
        ),
        134,
    );
}

#[test]
fn open_handler_arguments_are_strict() {
    let text = source(MIXED, INNER, OUTER, "let unused = handled in 42")
        .replace("foldOpen 0", "foldOpen (intQuot 1 0)");
    execute("open-strict", &text, 134);
}

#[test]
fn operation_names_do_not_affect_open_lowering() {
    let text = source(MIXED, INNER, OUTER, "intAdd result._2._2 1")
        .replace("UF", "First")
        .replace("UG", "Second")
        .replace(" F ", " Query ")
        .replace("(F ", "(Query ")
        .replace(" G ", " Reply ")
        .replace("(G ", "(Reply ");
    execute("open-renamed", &text, 42);
}

#[test]
fn collection_preserves_suspended_open_frames_and_captures() {
    let mut text = source(
        r"let saved = { offset: 2 } in C.bind (C.perform (UF (F \x -> x))) (\a -> C.return (intAdd a saved.offset))",
        r"let churn = C.run 0 noiseHandler (noise 5000) in { _1: intAdd s churn._1, _2: cont (k 40) }",
        OUTER,
        "if intEq result._2._1 5000 then result._2._2 else 1",
    );
    text.push_str(r"
noiseHandler :: forall x. Int -> G x -> (x -> C.Control G Int) -> { _1 :: Int, _2 :: C.Control G Int }
noiseHandler s (G k) cont = { _1: intAdd s 1, _2: cont (k s) }
noise :: Int -> C.Control G Int
noise n = if intEq n 0 then C.return 0 else C.bind (C.perform (G \x -> x)) (\_ -> noise (intSub n 1))");
    control_support::execute_with_gc("open-gc", &text, 42);
}

#[test]
fn library_state_fold_can_choose_when_to_forward_selected_requests() {
    let program = r"C.bind (C.perform (UF (F \x -> x))) (\a -> C.bind (C.perform (UF (F \x -> x))) (\b -> C.bind (C.perform (UF (F \x -> x))) (\c -> C.return (intAdd a (intAdd b c)))))";
    execute(
        "open-deep",
        &source(
            program,
            INNER,
            OUTER,
            "if intEq result._1 0 then if intEq result._2._1 3 then intAdd result._2._2 39 else 1 else 1",
        ),
        42,
    );
    execute(
        "open-shallow-library",
        &source(
            program,
            r"if intEq s 0 then { _1: 1, _2: cont (k 1) } else { _1: s, _2: C.bind (C.perform (UG (G \x -> x))) (\v -> cont (k v)) }",
            "{ _1: intAdd s 1, _2: cont (k 20) }",
            "if intEq result._1 2 then if intEq result._2._1 1 then intAdd result._2._2 1 else 1 else 1",
        ),
        42,
    );
}
