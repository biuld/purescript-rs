//! Required dispatch alternatives of the control machine, independent of IR.
use crate::control::schema::{ComputationCase, ContextCase, SequenceCase};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Carrier {
    Computation,
    Queue,
    Context,
    Reverse,
}

const COMPUTATION_TAGS: [i32; 6] = [
    ComputationCase::Return.tag() as i32,
    ComputationCase::Perform.tag() as i32,
    ComputationCase::Bind.tag() as i32,
    ComputationCase::Resume.tag() as i32,
    ComputationCase::Handle.tag() as i32,
    ComputationCase::OwnedResume.tag() as i32,
];

const QUEUE_TAGS: [i32; 4] = [
    SequenceCase::Empty.tag() as i32,
    SequenceCase::Cons.tag() as i32,
    SequenceCase::Append.tag() as i32,
    SequenceCase::Finish.tag() as i32,
];

const CONTEXT_TAGS: [i32; 2] = [
    ContextCase::Root.tag() as i32,
    ContextCase::Scoped.tag() as i32,
];

const REVERSE_TAGS: [i32; 3] = [
    SequenceCase::Empty.tag() as i32,
    SequenceCase::Cons.tag() as i32,
    SequenceCase::Finish.tag() as i32,
];

impl Carrier {
    pub(crate) fn tags(self) -> &'static [i32] {
        match self {
            Self::Computation => &COMPUTATION_TAGS,
            Self::Queue => &QUEUE_TAGS,
            Self::Context => &CONTEXT_TAGS,
            Self::Reverse => &REVERSE_TAGS,
        }
    }

    pub(crate) fn verify(self, tags: impl Iterator<Item = i32>) -> Result<(), &'static str> {
        let mut actual = tags.collect::<Vec<_>>();
        actual.sort_unstable();
        if actual != self.tags() {
            return Err("control dispatch must cover every required alternative exactly once");
        }
        Ok(())
    }
}
