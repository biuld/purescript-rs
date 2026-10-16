//! Complete action inventory from the operational semantics, independent of CC.
use super::schema::{ComputationCase as C, ContextCase as X, ExtentCase as E, SequenceCase as Q};
use super::schema::{ControlHelperRole as R, FieldKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Program {
    Executor,
    Run,
    CloneSequence,
    RebuildSequence,
    Check,
    Close,
    Start,
    Once,
    Cancel,
    RawResume,
}

impl Program {
    pub(crate) fn helper(self) -> Option<R> {
        Some(match self {
            Self::Executor => R::Executor,
            Self::Run => R::Run,
            Self::CloneSequence => R::CloneSequence,
            Self::RebuildSequence => R::RebuildSequence,
            Self::Check => R::Check,
            Self::Close => R::Close,
            Self::Start => R::Start,
            Self::Once => R::Once,
            Self::Cancel => R::Cancel,
            Self::RawResume => return None,
        })
    }

    pub(crate) fn parameters(self) -> &'static [FieldKind] {
        self.helper()
            .map_or(&[FieldKind::Callable, FieldKind::Payload], R::parameters)
    }

    pub(crate) fn result(self) -> FieldKind {
        self.helper().map_or(FieldKind::Payload, R::result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Advance,
    Publish,
    Return,
    Tail(R),
    Reject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Selection {
    Computation(C),
    Sequence(Q),
    Context(X),
    Extent(E),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ActionId {
    ReturnEmptyRoot,
    ReturnEmptyScoped,
    ReturnCons,
    ReturnFinish,
    AppendEmpty,
    AppendCons,
    AppendAppend,
    AppendFinish,
    PerformRoot,
    PerformScoped,
    Bind,
    Resume,
    HandleEmpty,
    HandleCons,
    HandleFinish,
    HandleAppendEmpty,
    HandleAppendCons,
    HandleAppendAppend,
    HandleAppendFinish,
    OwnedResume,
    CloneEmptyDone,
    CloneEmptyNext,
    CloneEmptyRejectCons,
    CloneEmptyRejectFinish,
    CloneCons,
    CloneAppend,
    CloneFinish,
    RebuildEmpty,
    RebuildCons,
    RebuildRejectAppend,
    RebuildFinish,
    CheckRoot,
    CheckScoped,
    CloseRoot,
    CloseScoped,
    Start,
    Once,
    Cancel,
    RawResume,
    Run,
}

impl ActionId {
    pub(crate) fn outcome(self) -> Outcome {
        match self {
            Self::ReturnEmptyRoot => Outcome::Publish,
            Self::CloneEmptyDone => Outcome::Tail(R::RebuildSequence),
            Self::CloneEmptyNext | Self::CloneCons | Self::CloneAppend | Self::CloneFinish => {
                Outcome::Tail(R::CloneSequence)
            }
            Self::CloneEmptyRejectCons
            | Self::CloneEmptyRejectFinish
            | Self::RebuildRejectAppend => Outcome::Reject,
            Self::RebuildCons | Self::RebuildFinish => Outcome::Tail(R::RebuildSequence),
            Self::CheckScoped => Outcome::Tail(R::Check),
            Self::RebuildEmpty
            | Self::CheckRoot
            | Self::CloseRoot
            | Self::CloseScoped
            | Self::Start
            | Self::Once
            | Self::Cancel
            | Self::RawResume
            | Self::Run => Outcome::Return,
            _ => Outcome::Advance,
        }
    }
    pub(crate) const ALL: [Self; 40] = [
        Self::ReturnEmptyRoot,
        Self::ReturnEmptyScoped,
        Self::ReturnCons,
        Self::ReturnFinish,
        Self::AppendEmpty,
        Self::AppendCons,
        Self::AppendAppend,
        Self::AppendFinish,
        Self::PerformRoot,
        Self::PerformScoped,
        Self::Bind,
        Self::Resume,
        Self::HandleEmpty,
        Self::HandleCons,
        Self::HandleFinish,
        Self::HandleAppendEmpty,
        Self::HandleAppendCons,
        Self::HandleAppendAppend,
        Self::HandleAppendFinish,
        Self::OwnedResume,
        Self::CloneEmptyDone,
        Self::CloneEmptyNext,
        Self::CloneEmptyRejectCons,
        Self::CloneEmptyRejectFinish,
        Self::CloneCons,
        Self::CloneAppend,
        Self::CloneFinish,
        Self::RebuildEmpty,
        Self::RebuildCons,
        Self::RebuildRejectAppend,
        Self::RebuildFinish,
        Self::CheckRoot,
        Self::CheckScoped,
        Self::CloseRoot,
        Self::CloseScoped,
        Self::Start,
        Self::Once,
        Self::Cancel,
        Self::RawResume,
        Self::Run,
    ];

    /// Every required action has one exact selection path in one program.
    pub(crate) fn selection(self) -> (Program, &'static [Selection]) {
        use Selection::{Computation as c, Context as x, Extent as e, Sequence as q};
        match self {
            Self::ReturnEmptyRoot => (Program::Executor, &[c(C::Return), q(Q::Empty), x(X::Root)]),
            Self::ReturnEmptyScoped => (
                Program::Executor,
                &[c(C::Return), q(Q::Empty), x(X::Scoped)],
            ),
            Self::ReturnCons => (Program::Executor, &[c(C::Return), q(Q::Cons)]),
            Self::ReturnFinish => (Program::Executor, &[c(C::Return), q(Q::Finish)]),
            Self::AppendEmpty => (
                Program::Executor,
                &[c(C::Return), q(Q::Append), q(Q::Empty)],
            ),
            Self::AppendCons => (Program::Executor, &[c(C::Return), q(Q::Append), q(Q::Cons)]),
            Self::AppendAppend => (
                Program::Executor,
                &[c(C::Return), q(Q::Append), q(Q::Append)],
            ),
            Self::AppendFinish => (
                Program::Executor,
                &[c(C::Return), q(Q::Append), q(Q::Finish)],
            ),
            Self::PerformRoot => (Program::Executor, &[c(C::Perform), x(X::Root)]),
            Self::PerformScoped => (Program::Executor, &[c(C::Perform), x(X::Scoped)]),
            Self::Bind => (Program::Executor, &[c(C::Bind)]),
            Self::Resume => (Program::Executor, &[c(C::Resume)]),
            Self::HandleEmpty => (Program::Executor, &[c(C::Handle), q(Q::Empty)]),
            Self::HandleCons => (Program::Executor, &[c(C::Handle), q(Q::Cons)]),
            Self::HandleFinish => (Program::Executor, &[c(C::Handle), q(Q::Finish)]),
            Self::HandleAppendEmpty => (
                Program::Executor,
                &[c(C::Handle), q(Q::Append), q(Q::Empty)],
            ),
            Self::HandleAppendCons => {
                (Program::Executor, &[c(C::Handle), q(Q::Append), q(Q::Cons)])
            }
            Self::HandleAppendAppend => (
                Program::Executor,
                &[c(C::Handle), q(Q::Append), q(Q::Append)],
            ),
            Self::HandleAppendFinish => (
                Program::Executor,
                &[c(C::Handle), q(Q::Append), q(Q::Finish)],
            ),
            Self::OwnedResume => (Program::Executor, &[c(C::OwnedResume)]),
            Self::CloneEmptyDone => (Program::CloneSequence, &[q(Q::Empty), q(Q::Empty)]),
            Self::CloneEmptyNext => (Program::CloneSequence, &[q(Q::Empty), q(Q::Append)]),
            Self::CloneEmptyRejectCons => (Program::CloneSequence, &[q(Q::Empty), q(Q::Cons)]),
            Self::CloneEmptyRejectFinish => (Program::CloneSequence, &[q(Q::Empty), q(Q::Finish)]),
            Self::CloneCons => (Program::CloneSequence, &[q(Q::Cons)]),
            Self::CloneAppend => (Program::CloneSequence, &[q(Q::Append)]),
            Self::CloneFinish => (Program::CloneSequence, &[q(Q::Finish)]),
            Self::RebuildEmpty => (Program::RebuildSequence, &[q(Q::Empty)]),
            Self::RebuildCons => (Program::RebuildSequence, &[q(Q::Cons)]),
            Self::RebuildRejectAppend => (Program::RebuildSequence, &[q(Q::Append)]),
            Self::RebuildFinish => (Program::RebuildSequence, &[q(Q::Finish)]),
            Self::CheckRoot => (Program::Check, &[e(E::Root)]),
            Self::CheckScoped => (Program::Check, &[e(E::Scoped)]),
            Self::CloseRoot => (Program::Close, &[e(E::Root)]),
            Self::CloseScoped => (Program::Close, &[e(E::Scoped)]),
            Self::Start => (Program::Start, &[]),
            Self::Once => (Program::Once, &[]),
            Self::Cancel => (Program::Cancel, &[]),
            Self::RawResume => (Program::RawResume, &[]),
            Self::Run => (Program::Run, &[]),
        }
    }
}
