//! Replay nested target finish frames without consuming suspended checkpoints.
mod control_support;
use control_support::execute;

const SOURCE: &str = include_str!("control_support/scoped_replay.purs");

#[test]
fn two_replays_preserve_nested_suspended_source_checkpoints() {
    execute("scoped-nested-replay", &SOURCE.replace("AFTER", r"C.bind (raw (k 0)) (\b -> C.return { saved: a.saved, built: a.built, value: intAdd a.value b.value })"), 42);
}

#[test]
fn captures_created_in_a_replay_expire_at_that_branchs_completion() {
    for (name, value) in [("call", "a.saved 42"), ("node", "a.built")] {
        let after = format!(
            "let expired = C.run 0 closed ({value}) in C.return {{ saved: a.saved, built: a.built, value: expired._2 }}"
        );
        execute(
            &format!("scoped-replay-expired-{name}"),
            &SOURCE.replace("AFTER", &after),
            134,
        );
    }
}
