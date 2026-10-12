//! Actual source context construction and entry operands must retain identity.
mod aliases;
mod cc;
mod mir;
use super::control_checkpoints::{SOURCE, binding};
use psrs_backend::cc::ControlHelperRole as R;

#[derive(Clone, Copy)]
enum Mutation {
    Clauses,
    Caller,
    TailQueue,
    TailParent,
    TailExtent,
    EntryBody,
    EntryQueue,
    EntryExtent,
    EntryContext,
    Status,
    StatusOutsideLoop,
}
impl Mutation {
    fn message(self) -> &'static str {
        match self {
            Self::Caller => "retain its current caller context",
            Self::Clauses => "retain the current Handle return and operation clauses",
            Self::TailQueue | Self::TailParent => {
                "retain queue, parent and extent from its current finish"
            }
            Self::TailExtent => "save its current queue and execution parent",
            Self::Status | Self::StatusOutsideLoop => "fresh live status cell in its iteration",
            _ => "enter its Handle body with an empty queue and its own extent",
        }
    }
}

#[test]
fn cc_source_context_retains_clauses() {
    cc::reject(Mutation::Clauses);
}

#[test]
fn mir_source_context_retains_clauses() {
    mir::reject(Mutation::Clauses);
}

#[test]
fn cc_source_context_retains_tail_queue() {
    cc::reject(Mutation::TailQueue);
}

#[test]
fn mir_source_context_retains_tail_queue() {
    mir::reject(Mutation::TailQueue);
}

#[test]
fn cc_source_context_retains_tail_parent() {
    cc::reject(Mutation::TailParent);
}

#[test]
fn mir_source_context_retains_tail_parent() {
    mir::reject(Mutation::TailParent);
}

#[test]
fn cc_source_context_retains_tail_extent() {
    cc::reject(Mutation::TailExtent);
}

#[test]
fn mir_source_context_retains_tail_extent() {
    mir::reject(Mutation::TailExtent);
}

#[test]
fn cc_source_context_retains_entry_body() {
    cc::reject(Mutation::EntryBody);
}

#[test]
fn mir_source_context_retains_entry_body() {
    mir::reject(Mutation::EntryBody);
}

#[test]
fn cc_source_context_retains_entry_queue() {
    cc::reject(Mutation::EntryQueue);
}

#[test]
fn mir_source_context_retains_entry_queue() {
    mir::reject(Mutation::EntryQueue);
}

#[test]
fn cc_source_context_retains_entry_extent() {
    cc::reject(Mutation::EntryExtent);
}

#[test]
fn mir_source_context_retains_entry_extent() {
    mir::reject(Mutation::EntryExtent);
}

#[test]
fn cc_source_context_retains_entry_context() {
    cc::reject(Mutation::EntryContext);
}

#[test]
fn mir_source_context_retains_entry_context() {
    mir::reject(Mutation::EntryContext);
}

#[test]
fn cc_source_context_retains_fresh_status() {
    cc::reject(Mutation::Status);
}

#[test]
fn mir_source_context_retains_fresh_status() {
    mir::reject(Mutation::Status);
}

#[test]
fn cc_source_context_retains_current_caller() {
    cc::reject(Mutation::Caller);
}
#[test]
fn mir_source_context_retains_current_caller() {
    mir::reject(Mutation::Caller);
}

#[test]
fn mir_source_context_status_is_allocated_inside_the_executor_loop() {
    mir::reject(Mutation::StatusOutsideLoop);
}
