//! Root initialization and normal cleanup must belong to the checked Run.
mod cc;
mod mir;
mod ownership;
mod paths;
use super::control_checkpoints::{SOURCE, binding};
use psrs_backend::cc::ControlHelperRole as R;

#[derive(Clone, Copy)]
enum Mutation {
    DispatchExtent,
    ExecutionExtent,
    Queue,
    MissingClose,
    WrongClose,
    EarlyClose,
    CheckInsteadClose,
    StartStatus,
}
impl Mutation {
    fn message(self) -> &'static str {
        match self {
            Self::DispatchExtent | Self::ExecutionExtent => {
                "same fresh root extent for execution and dispatch"
            }
            Self::Queue => "start with an empty root queue",
            Self::StartStatus => "fresh one-element live status cell",
            _ => "close its root after execution and before publishing its result",
        }
    }
}

#[test]
fn cc_rejects_incorrect_root_dispatch_extent() {
    cc::reject(Mutation::DispatchExtent);
}

#[test]
fn mir_rejects_incorrect_root_dispatch_extent() {
    mir::reject(Mutation::DispatchExtent);
}

#[test]
fn cc_rejects_incorrect_root_execution_extent() {
    cc::reject(Mutation::ExecutionExtent);
}

#[test]
fn mir_rejects_incorrect_root_execution_extent() {
    mir::reject(Mutation::ExecutionExtent);
}

#[test]
fn cc_rejects_incorrect_root_queue() {
    cc::reject(Mutation::Queue);
}

#[test]
fn mir_rejects_incorrect_root_queue() {
    mir::reject(Mutation::Queue);
}

#[test]
fn cc_rejects_incorrect_root_missing_close() {
    cc::reject(Mutation::MissingClose);
}

#[test]
fn mir_rejects_incorrect_root_missing_close() {
    mir::reject(Mutation::MissingClose);
}

#[test]
fn cc_rejects_incorrect_root_wrong_close() {
    cc::reject(Mutation::WrongClose);
}

#[test]
fn mir_rejects_incorrect_root_wrong_close() {
    mir::reject(Mutation::WrongClose);
}

#[test]
fn cc_rejects_incorrect_root_early_close() {
    cc::reject(Mutation::EarlyClose);
}

#[test]
fn mir_rejects_incorrect_root_early_close() {
    mir::reject(Mutation::EarlyClose);
}

#[test]
fn cc_rejects_incorrect_root_check_instead_of_close() {
    cc::reject(Mutation::CheckInsteadClose);
}

#[test]
fn mir_rejects_incorrect_root_check_instead_of_close() {
    mir::reject(Mutation::CheckInsteadClose);
}

#[test]
fn cc_rejects_incorrect_root_start_status() {
    cc::reject(Mutation::StartStatus);
}

#[test]
fn mir_rejects_incorrect_root_start_status() {
    mir::reject(Mutation::StartStatus);
}
