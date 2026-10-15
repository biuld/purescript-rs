//! The authoritative named schema of the control machine.
//!
//! This module names the machine's node cases, their fields and the fixed
//! helper roles without importing any IR representation. Physical layout
//! planning, P8 construction and every verifier consume these names instead of
//! positional offsets, so a case field or a role is stated once. The schema is
//! effect-agnostic: it never mentions a concrete operation or handler.
//!
//! A named case is always identified by its stable tag. Field index constants
//! are the machine's own, not an inference from an IR variant's field order;
//! layout planning checks that the physical frame it emits has exactly the
//! declared arity.

/// Logical storage roles, before any CC representation or physical MIR layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldKind {
    Payload,
    Callable,
    Computation,
    Sequence,
    Context,
    Extent,
    Status,
    Owner,
    Callback,
    ResultPair,
    ReplayResult,
}

/// Direct-call helper roles. Raw resume uses the closure receiver protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlHelperRole {
    Executor,
    Run,
    CloneSequence,
    RebuildSequence,
    Start,
    Check,
    Close,
    Once,
    Cancel,
}

impl ControlHelperRole {
    pub(crate) const ALL: [Self; 9] = [
        Self::Executor,
        Self::Run,
        Self::CloneSequence,
        Self::RebuildSequence,
        Self::Start,
        Self::Check,
        Self::Close,
        Self::Once,
        Self::Cancel,
    ];

    pub(crate) fn parameters(self) -> &'static [FieldKind] {
        use FieldKind::*;
        match self {
            Self::Run => &[Payload, Callable, Computation],
            Self::Executor => &[Payload, Callable, Computation, Sequence, Extent, Context],
            Self::CloneSequence => &[Sequence, Sequence, Sequence, Extent, Extent],
            Self::RebuildSequence => &[Sequence, Sequence, Extent, Extent],
            Self::Start => &[],
            Self::Check | Self::Close => &[Extent],
            Self::Once => &[Callable],
            Self::Cancel => &[Owner],
        }
    }

    pub(crate) fn result(self) -> FieldKind {
        use FieldKind::*;
        match self {
            Self::Executor | Self::Run => Payload,
            Self::CloneSequence | Self::RebuildSequence => ReplayResult,
            Self::Start | Self::Check | Self::Close => Extent,
            Self::Once | Self::Cancel => Owner,
        }
    }
}

/// The six cases of a `Control f a` computation frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComputationCase {
    Return,
    Perform,
    Bind,
    Resume,
    Handle,
    OwnedResume,
}

impl ComputationCase {
    pub(crate) fn fields(self) -> &'static [FieldKind] {
        use FieldKind::*;
        match self {
            Self::Return | Self::Perform => &[Payload],
            Self::Bind => &[Computation, Callable],
            Self::Resume => &[Payload, Sequence, Extent],
            Self::Handle => &[Callable, Callable, Computation],
            Self::OwnedResume => &[Owner, Payload],
        }
    }
    pub const ALL: [Self; 6] = [
        Self::Return,
        Self::Perform,
        Self::Bind,
        Self::Resume,
        Self::Handle,
        Self::OwnedResume,
    ];

    pub const fn tag(self) -> u32 {
        self as u32
    }
}

/// Fields of the computation cases whose payload is not a single erased value.
pub mod computation {
    /// `Bind`: the inner computation and the callback it feeds.
    pub const BIND_INNER: u32 = 0;
    pub const BIND_CONTINUATION: u32 = 1;
    /// `Return`/`Perform`: the erased source payload.
    pub const PAYLOAD: u32 = 0;
    /// `Resume`: the erased answer, the saved source queue, the checkpoint.
    pub const RESUME_VALUE: u32 = 0;
    pub const RESUME_SAVED: u32 = 1;
    pub const RESUME_CHECKPOINT: u32 = 2;
    /// `Handle`: the return clause, the operation clause, the source body.
    pub const HANDLE_RETURN: u32 = 0;
    pub const HANDLE_OPERATION: u32 = 1;
    pub const HANDLE_SOURCE: u32 = 2;
    /// `OwnedResume`: the one-shot owner and the erased answer.
    pub const OWNED_OWNER: u32 = 0;
    pub const OWNED_VALUE: u32 = 1;
}

/// The four cases of the persistent sequencing spine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SequenceCase {
    Empty,
    Cons,
    Append,
    Finish,
}

impl SequenceCase {
    pub(crate) fn fields(self) -> &'static [FieldKind] {
        use FieldKind::*;
        match self {
            Self::Empty => &[],
            Self::Cons => &[Callable, Sequence],
            Self::Append => &[Sequence, Sequence],
            Self::Finish => &[Extent, Sequence, Extent],
        }
    }
    pub const ALL: [Self; 4] = [Self::Empty, Self::Cons, Self::Append, Self::Finish];

    pub const fn tag(self) -> u32 {
        self as u32
    }
}

/// Fields of the sequencing spine.
pub mod sequence {
    pub const CONS_HEAD: u32 = 0;
    pub const CONS_TAIL: u32 = 1;
    pub const APPEND_LEFT: u32 = 0;
    pub const APPEND_RIGHT: u32 = 1;
    pub const FINISH_EXTENT: u32 = 0;
    pub const FINISH_REST: u32 = 1;
    pub const FINISH_PARENT: u32 = 2;
}

/// The two cases of a dispatch context.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContextCase {
    Root,
    Scoped,
}

impl ContextCase {
    pub(crate) fn fields(self) -> &'static [FieldKind] {
        use FieldKind::*;
        match self {
            Self::Root => &[Extent],
            Self::Scoped => &[Callable, Callable, Sequence, Extent, Context, Extent],
        }
    }
    pub const ALL: [Self; 2] = [Self::Root, Self::Scoped];

    pub const fn tag(self) -> u32 {
        self as u32
    }
}

/// Fields of a dispatch context.
pub mod context {
    pub const ROOT_EXTENT: u32 = 0;
    pub const SCOPED_RETURN: u32 = 0;
    pub const SCOPED_OPERATION: u32 = 1;
    pub const SCOPED_SAVED: u32 = 2;
    pub const SCOPED_PARENT: u32 = 3;
    pub const SCOPED_CALLER: u32 = 4;
    pub const SCOPED_EXTENT: u32 = 5;
}

/// The two cases of a lifetime extent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExtentCase {
    Root,
    Scoped,
}

impl ExtentCase {
    pub(crate) fn fields(self) -> &'static [FieldKind] {
        use FieldKind::*;
        match self {
            Self::Root => &[Status],
            Self::Scoped => &[Status, Extent],
        }
    }
    pub const ALL: [Self; 2] = [Self::Root, Self::Scoped];

    pub const fn tag(self) -> u32 {
        self as u32
    }
}

/// Fields of a lifetime extent.
pub mod extent {
    pub const STATUS: u32 = 0;
    pub const PARENT: u32 = 1;
}

/// The single case of a one-shot owner object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OwnerCase {
    OneShot,
}

impl OwnerCase {
    pub(crate) fn fields(self) -> &'static [FieldKind] {
        &[FieldKind::Callback, FieldKind::Status]
    }
    pub const ALL: [Self; 1] = [Self::OneShot];

    pub const fn tag(self) -> u32 {
        self as u32
    }
}

/// Fields of a one-shot owner object.
pub mod owner {
    pub const CALLBACK: u32 = 0;
    pub const STATUS: u32 = 1;
}

/// Fields of the closed-run result pair.
pub mod result_pair {
    pub const STATE: u32 = 0;
    pub const VALUE: u32 = 1;
}

/// Fields of a replay result.
pub mod replay {
    pub const QUEUE: u32 = 0;
    pub const ACTIVE: u32 = 1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_tags_are_stable_and_unique() {
        for (index, case) in ComputationCase::ALL.iter().enumerate() {
            assert_eq!(case.tag(), index as u32);
        }
        for (index, case) in SequenceCase::ALL.iter().enumerate() {
            assert_eq!(case.tag(), index as u32);
        }
        for (index, case) in ContextCase::ALL.iter().enumerate() {
            assert_eq!(case.tag(), index as u32);
        }
        for (index, case) in ExtentCase::ALL.iter().enumerate() {
            assert_eq!(case.tag(), index as u32);
        }
        assert_eq!(OwnerCase::OneShot.tag(), 0);
    }
}
