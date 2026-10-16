//! Runtime consumption of one-shot owners; construction remains inert.
mod control_support;
use control_support::execute;

fn source(body: &str) -> String {
    let body = body.replace("; ", " in let ");
    format!(
        r#"module Main where
import Prim.Control as C
data Op next = Op (Int -> next)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> {{ _1 :: Int, _2 :: C.Control Op Int }}
handler s (Op k) cont = {{ _1: s, _2: cont (k s) }}
main :: Int
main = {body}
"#
    )
}

#[test]
fn a_one_shot_owner_runs_once() {
    execute(
        "owned-once",
        &source(
            r"let owner = C.once (\n -> C.return (intAdd n 2)) in (C.run 0 handler (C.resume owner 40))._2",
        ),
        42,
    );
}

#[test]
fn building_an_unused_resume_does_not_consume_ownership() {
    execute(
        "owned-inert",
        &source(
            r"let owner = C.once (\n -> C.return n); unused = C.resume owner 7 in (C.run 0 handler (C.resume owner 42))._2",
        ),
        42,
    );
}

#[test]
fn executing_two_resume_nodes_rejects_the_second() {
    execute(
        "owned-double",
        &source(
            r"let owner = C.once (\n -> C.return n) in (C.run 0 handler (C.bind (C.resume owner 19) (\a -> C.bind (C.resume owner 23) (\b -> C.return (intAdd a b)))))._2",
        ),
        134,
    );
}

#[test]
fn repeating_the_same_resume_node_rejects_reinterpretation() {
    execute(
        "owned-repeat",
        &source(
            r"let owner = C.once (\n -> C.return n); program = C.resume owner 21; first = C.run 0 handler program; second = C.run 0 handler program in intAdd first._2 second._2",
        ),
        134,
    );
}

#[test]
fn cancelling_a_pending_owner_rejects_resume() {
    execute(
        "owned-cancel",
        &source(
            r"let owner = C.once (\n -> C.return n); cancelled = C.cancel owner in (C.run 0 handler (C.resume owner 42))._2",
        ),
        134,
    );
}

#[test]
fn cancellation_is_single_use() {
    execute(
        "owned-cancel-twice",
        &source(
            r"let owner = C.once (\n -> C.return n); first = C.cancel owner; second = C.cancel owner in 42",
        ),
        134,
    );
}

#[test]
fn a_consumed_owner_cannot_be_cancelled() {
    execute(
        "owned-cancel-consumed",
        &source(
            r"let owner = C.once (\n -> C.return n); first = C.run 0 handler (C.resume owner 42); cancelled = C.cancel owner in first._2",
        ),
        134,
    );
}

#[test]
fn resume_factory_arguments_are_strict() {
    execute(
        "owned-strict",
        &source(
            r"let owner = C.once (\n -> C.return n); unused = C.resume owner (intQuot 1 0) in 42",
        ),
        134,
    );
}

#[test]
fn reentrant_interpretation_rejects_a_running_owner() {
    execute(
        "owned-reentrant",
        &source(
            r"let holder = [C.once (\n -> C.return n)]; owner = C.once (\n -> let nested = C.run 0 handler (C.resume (arrayIndex holder 0) n) in C.return nested._2); written = arrayWrite holder 0 owner in (C.run 0 handler (C.resume owner 42))._2",
        ),
        134,
    );
}

#[test]
fn reentrant_cancellation_rejects_a_running_owner() {
    execute(
        "owned-reentrant-cancel",
        &source(
            r"let holder = [C.once (\n -> C.return n)]; owner = C.once (\n -> let cancelled = C.cancel (arrayIndex holder 0) in C.return n); written = arrayWrite holder 0 owner in (C.run 0 handler (C.resume owner 42))._2",
        ),
        134,
    );
}

#[test]
fn separate_owners_do_not_share_consumption_state() {
    execute(
        "owned-independent",
        &source(
            r"let first = C.once (\n -> C.return n); second = C.once (\n -> C.return n) in (C.run 0 handler (C.bind (C.resume first 19) (\a -> C.bind (C.resume second 23) (\b -> C.return (intAdd a b)))))._2",
        ),
        42,
    );
}

#[test]
fn an_ignored_one_shot_interpretation_keeps_its_callback_trap() {
    execute(
        "owned-ignored",
        &source(
            r"let owner = C.once (\_ -> C.return (intQuot 1 0)); unused = C.run 0 handler (C.resume owner 42) in 42",
        ),
        134,
    );
}

#[test]
fn an_abandoned_resume_node_does_not_consume_its_owner() {
    execute(
        "owned-abandoned",
        &source(
            r"let owner = C.once (\n -> C.return n); program = C.bind (C.perform (Op \x -> x)) (\_ -> C.resume owner 7); skipped = C.run 0 (\s op cont -> { _1: s, _2: C.return 0 }) program in intAdd skipped._2 (C.run 0 handler (C.resume owner 42))._2",
        ),
        42,
    );
}

#[test]
fn collection_preserves_a_pending_one_shot_owners_captures() {
    let mut text = source(
        r"let saved = { offset: 2 }; owner = C.once (\n -> C.return (intAdd n saved.offset)); churn = C.run 0 handler (noise 5000) in if intEq churn._2 0 then (C.run 0 handler (C.resume owner 40))._2 else 1",
    );
    text.push_str(r"
noise :: Int -> C.Control Op Int
noise n = if intEq n 0 then C.return 0 else C.bind (C.perform (Op \x -> x)) (\_ -> noise (intSub n 1))");
    control_support::execute_with_gc("owned-gc", &text, 42);
}
