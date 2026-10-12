//! Lifecycle replay must preserve checkpoints and allocate distinct activations.
mod allocation;
mod cc;
mod mir;
use super::control_checkpoints::{SOURCE, binding};
use psrs_backend::cc::ControlHelperRole as R;

#[derive(Clone, Copy)]
enum Mutation {
    WrongSavedCheck,
    PrematureClose,
    CallbackFromOtherQueue,
    MissingSavedCheck,
    ReusedActivation,
    WrongActivationParent,
    WrongRestoredParent,
    ClosedStatus,
    WrongAdvance,
    StatusOutsideLoop,
}

impl Mutation {
    fn role(self) -> R {
        match self {
            Self::WrongSavedCheck
            | Self::MissingSavedCheck
            | Self::PrematureClose
            | Self::CallbackFromOtherQueue => R::CloneSequence,
            _ => R::RebuildSequence,
        }
    }
    fn message(self) -> &'static str {
        match self {
            Self::PrematureClose => {
                "replay traversal cannot invoke callbacks or mutate lifecycle cells"
            }
            Self::CallbackFromOtherQueue => {
                "share the original callback and retain its accumulator"
            }
            Self::WrongSavedCheck | Self::MissingSavedCheck => {
                "check the saved finish extent before copying it"
            }
            Self::ReusedActivation => "create a fresh activation extent",
            Self::WrongActivationParent => "activation must use its current parent",
            Self::WrongRestoredParent => "retain its accumulated queue and previous active extent",
            Self::ClosedStatus | Self::StatusOutsideLoop => {
                "fresh live status cell in its iteration"
            }
            Self::WrongAdvance => "advance its queue, active extent and parent together",
        }
    }
}

#[test]
fn cc_checks_original_saved_extent() {
    cc::reject(Mutation::WrongSavedCheck);
}
#[test]
fn mir_checks_original_saved_extent() {
    mir::reject(Mutation::WrongSavedCheck);
}
#[test]
fn cc_requires_saved_extent_check() {
    cc::reject(Mutation::MissingSavedCheck);
}
#[test]
fn mir_requires_saved_extent_check() {
    mir::reject(Mutation::MissingSavedCheck);
}
#[test]
fn cc_rejects_reusing_an_activation() {
    cc::reject(Mutation::ReusedActivation);
}
#[test]
fn mir_rejects_reusing_an_activation() {
    mir::reject(Mutation::ReusedActivation);
}
#[test]
fn cc_checks_fresh_activation_parent() {
    cc::reject(Mutation::WrongActivationParent);
}
#[test]
fn mir_checks_fresh_activation_parent() {
    mir::reject(Mutation::WrongActivationParent);
}
#[test]
fn cc_checks_restored_active_extent() {
    cc::reject(Mutation::WrongRestoredParent);
}
#[test]
fn mir_checks_restored_active_extent() {
    mir::reject(Mutation::WrongRestoredParent);
}
#[test]
fn cc_requires_a_live_new_status() {
    cc::reject(Mutation::ClosedStatus);
}
#[test]
fn mir_requires_a_live_new_status() {
    mir::reject(Mutation::ClosedStatus);
}
#[test]
fn cc_checks_activation_advance() {
    cc::reject(Mutation::WrongAdvance);
}
#[test]
fn mir_checks_activation_advance() {
    mir::reject(Mutation::WrongAdvance);
}
#[test]
fn mir_rejects_status_allocation_before_the_rebuild_loop() {
    mir::reject(Mutation::StatusOutsideLoop);
}

#[test]
fn cc_replay_cannot_complete_a_saved_extent() {
    cc::reject(Mutation::PrematureClose);
}
#[test]
fn mir_replay_cannot_complete_a_saved_extent() {
    mir::reject(Mutation::PrematureClose);
}
#[test]
fn cc_replay_must_share_the_original_callback() {
    cc::reject(Mutation::CallbackFromOtherQueue);
}
#[test]
fn mir_replay_must_share_the_original_callback() {
    mir::reject(Mutation::CallbackFromOtherQueue);
}
