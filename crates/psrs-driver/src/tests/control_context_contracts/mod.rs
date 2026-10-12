//! Verify completion context creation and replay ancestry seeding/propagation.
mod aliases;
mod cc;
mod mir;
use super::control_checkpoints::{SOURCE, binding};
use psrs_backend::cc::ControlHelperRole as R;

#[derive(Clone, Copy)]
enum Mutation {
    FinishExtent,
    FinishQueue,
    FinishParent,
    CallerContext,
    CopyParent,
    CopyOrder,
    SeedParent,
    SeedActive,
    SeedAccumulator,
    CloneForward,
    CallbackForward,
}
impl Mutation {
    fn role(self) -> R {
        match self {
            Self::CloneForward => R::CloneSequence,
            Self::CallbackForward => R::RebuildSequence,
            _ => R::Executor,
        }
    }
    fn message(self) -> &'static str {
        match self {
            Self::FinishExtent => "finish copy must preserve its original extent",
            Self::FinishQueue | Self::FinishParent => {
                "target finish must preserve its source context"
            }
            Self::CallerContext => "enter its caller context with the saved extent",
            Self::CopyParent => "finish copy must preserve its original parent",
            Self::CopyOrder => "finish copy must reassociate the original queue",
            Self::SeedParent => "replay parent must select its current dispatch context",
            Self::SeedActive => "start with its current execution extent",
            Self::SeedAccumulator => "start with empty traversal accumulators",
            Self::CloneForward => {
                "clone traversal must preserve its accumulators, active extent and parent"
            }
            Self::CallbackForward => "callback rebuild must preserve its active extent and parent",
        }
    }
}
#[test]
fn cc_target_finish_uses_its_context_extent() {
    cc::reject(Mutation::FinishExtent);
}
#[test]
fn mir_target_finish_uses_its_context_extent() {
    mir::reject(Mutation::FinishExtent);
}
#[test]
fn cc_target_finish_uses_its_context_queue() {
    cc::reject(Mutation::FinishQueue);
}
#[test]
fn mir_target_finish_uses_its_context_queue() {
    mir::reject(Mutation::FinishQueue);
}
#[test]
fn cc_target_finish_uses_its_context_parent() {
    cc::reject(Mutation::FinishParent);
}
#[test]
fn mir_target_finish_uses_its_context_parent() {
    mir::reject(Mutation::FinishParent);
}
#[test]
fn cc_target_finish_enters_its_caller_context() {
    cc::reject(Mutation::CallerContext);
}
#[test]
fn mir_target_finish_enters_its_caller_context() {
    mir::reject(Mutation::CallerContext);
}
#[test]
fn cc_finish_copy_preserves_its_parent() {
    cc::reject(Mutation::CopyParent);
}
#[test]
fn mir_finish_copy_preserves_its_parent() {
    mir::reject(Mutation::CopyParent);
}
#[test]
fn cc_finish_copy_preserves_append_order() {
    cc::reject(Mutation::CopyOrder);
}
#[test]
fn mir_finish_copy_preserves_append_order() {
    mir::reject(Mutation::CopyOrder);
}
#[test]
fn cc_replay_parent_comes_from_dispatch() {
    cc::reject(Mutation::SeedParent);
}
#[test]
fn mir_replay_parent_comes_from_dispatch() {
    mir::reject(Mutation::SeedParent);
}
#[test]
fn cc_replay_starts_with_current_active_extent() {
    cc::reject(Mutation::SeedActive);
}
#[test]
fn mir_replay_starts_with_current_active_extent() {
    mir::reject(Mutation::SeedActive);
}
#[test]
fn cc_replay_starts_with_empty_accumulators() {
    cc::reject(Mutation::SeedAccumulator);
}
#[test]
fn mir_replay_starts_with_empty_accumulators() {
    mir::reject(Mutation::SeedAccumulator);
}
#[test]
fn cc_clone_preserves_parent_during_traversal() {
    cc::reject(Mutation::CloneForward);
}
#[test]
fn mir_clone_preserves_parent_during_traversal() {
    mir::reject(Mutation::CloneForward);
}
#[test]
fn cc_callback_rebuild_preserves_parent() {
    cc::reject(Mutation::CallbackForward);
}
#[test]
fn mir_callback_rebuild_preserves_parent() {
    mir::reject(Mutation::CallbackForward);
}
