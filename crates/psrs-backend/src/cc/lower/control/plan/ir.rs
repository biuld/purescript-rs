//! The plan's named machine IR and construction helpers.

use crate::cc::layout::control::ControlLayout;
use crate::cc::{ControlHelperRole, ReprId};
use crate::control::schema::{ComputationCase, ContextCase, ExtentCase, OwnerCase, SequenceCase};

/// A named executor parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Value,
    Handler,
    Computation,
    Sequence,
    Extent,
    Context,
}

pub(crate) fn state_index(state: State) -> usize {
    match state {
        State::Value => 0,
        State::Handler => 1,
        State::Computation => 2,
        State::Sequence => 3,
        State::Extent => 4,
        State::Context => 5,
    }
}

/// A value available to a transition.
#[derive(Clone, Debug)]
pub(crate) enum Operand {
    State(State),
    /// A caller-supplied parameter of a helper body.
    Param(usize),
    /// A result bound earlier in the enclosing scope.
    Temp(usize),
}

pub(crate) fn state(slot: State) -> Operand {
    Operand::State(slot)
}

/// The machine case family a projection or allocation belongs to.
/// The machine carrier a case belongs to, without its inner case value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Family {
    Computation,
    Sequence,
    Context,
    Extent,
    Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Case {
    Computation(ComputationCase),
    Sequence(SequenceCase),
    Context(ContextCase),
    Extent(ExtentCase),
    Owner(OwnerCase),
}

impl Case {
    pub(crate) fn family(self) -> Family {
        match self {
            Self::Computation(_) => Family::Computation,
            Self::Sequence(_) => Family::Sequence,
            Self::Context(_) => Family::Context,
            Self::Extent(_) => Family::Extent,
            Self::Owner(_) => Family::Owner,
        }
    }

    pub(crate) fn repr(self, layout: ControlLayout) -> ReprId {
        match self {
            Self::Computation(_) => layout.computation,
            Self::Sequence(_) => layout.sequence,
            Self::Context(_) => layout.context,
            Self::Extent(_) => layout.boundary,
            Self::Owner(_) => layout.owner,
        }
    }

    pub(crate) fn tag(self) -> u32 {
        match self {
            Self::Computation(case) => case.tag(),
            Self::Sequence(case) => case.tag(),
            Self::Context(case) => case.tag(),
            Self::Extent(case) => case.tag(),
            Self::Owner(case) => case.tag(),
        }
    }

    pub(crate) fn field_count(self) -> usize {
        match self {
            Self::Computation(case) => case.field_count(),
            Self::Sequence(case) => case.field_count(),
            Self::Context(case) => case.field_count(),
            Self::Extent(case) => case.field_count(),
            Self::Owner(case) => case.field_count(),
        }
    }

    /// The cases a dispatch on this family must cover.
    pub(crate) fn tags(self) -> Vec<u32> {
        match self {
            Self::Computation(_) => ComputationCase::ALL.iter().map(|c| c.tag()).collect(),
            Self::Sequence(_) => SequenceCase::ALL.iter().map(|c| c.tag()).collect(),
            Self::Context(_) => ContextCase::ALL.iter().map(|c| c.tag()).collect(),
            Self::Extent(_) => ExtentCase::ALL.iter().map(|c| c.tag()).collect(),
            Self::Owner(_) => OwnerCase::ALL.iter().map(|c| c.tag()).collect(),
        }
    }
}

/// One ordered construction step. Every step binds a result so an effectful step
/// is sequenced before the outcome even when its result is unused.
#[derive(Clone, Debug)]
pub(crate) enum Op {
    Project {
        source: Operand,
        case: Case,
        field: u32,
    },
    Node {
        case: Case,
        fields: Vec<Operand>,
    },
    /// Read a capture slot of a closure; `case` names the expected shape.
    Capture {
        closure: Operand,
        index: u32,
        case: Case,
    },
    Apply {
        callee: Operand,
        args: Vec<Operand>,
    },
    Helper {
        role: ControlHelperRole,
        args: Vec<Operand>,
    },
    ResumeClosure {
        captures: Vec<Operand>,
    },
    Pair {
        state: Operand,
        value: Operand,
    },
    PairField {
        pair: Operand,
        field: u32,
    },
    ReplayField {
        result: Operand,
        field: u32,
    },
    FreshExtent {
        parent: Option<Operand>,
    },
    ContextExtent {
        context: Operand,
    },
    CheckBoundary {
        extent: Operand,
    },
    CloseBoundary {
        extent: Operand,
    },
    GuardPending {
        cell: Operand,
    },
    SetStatus {
        cell: Operand,
        value: i32,
    },
    ReadCallback {
        cell: Operand,
    },
    WriteCallback {
        cell: Operand,
        value: Operand,
    },
    /// Allocate the one-element callback cell holding a reference value.
    NewCallbackCell {
        value: Operand,
    },
    /// Allocate a fresh one-element integer status cell initialised to zero.
    NewStatus,
    /// Allocate a planned product of erased fields.
    Product {
        product: Product,
        fields: Vec<Operand>,
    },
    EmptySequence,
}

/// A program over the machine's named case refinements.
#[derive(Clone, Debug)]
pub(crate) enum Exec {
    /// Bind each operation's result in order, then continue.
    Steps { ops: Vec<Op>, next: Box<Exec> },
    /// Refine one carrier by case; every schema case has exactly one arm.
    Switch {
        source: Operand,
        case: Case,
        arms: Vec<(u32, Exec)>,
    },
    /// Continue the executor with a complete named state.
    Advance {
        state: Operand,
        handler: Operand,
        computation: Operand,
        sequence: Operand,
        extent: Operand,
        context: Operand,
    },
    /// Return the published result pair.
    Publish { result: Operand },
    /// Return one value from a helper body.
    Return { value: Operand },
    /// Call a helper as the body's final action and return its result.
    Tail {
        role: ControlHelperRole,
        args: Vec<Operand>,
    },
    /// Trap with a value of the given planned shape, matching a sibling arm.
    Reject { shape: Shape },
}

/// A planned representation the builder names without a concrete [`ReprId`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Product {
    ReplayResult,
}

/// The shape of a rejected arm, named without a concrete representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    ReplayResult,
}

/// Assigns temporary indices in emission order. A scope's arms share the
/// cursor position they start from, matching the cloned environment the
/// expander gives each arm.
#[derive(Clone)]
pub(crate) struct Cursor {
    next: usize,
}

impl Cursor {
    pub(crate) fn new() -> Self {
        Self { next: 0 }
    }

    pub(crate) fn bind(&mut self, ops: &mut Vec<Op>, op: Op) -> Operand {
        let operand = Operand::Temp(self.next);
        ops.push(op);
        self.next += 1;
        operand
    }
}

pub(crate) fn steps(ops: Vec<Op>, next: Exec) -> Exec {
    Exec::Steps {
        ops,
        next: Box::new(next),
    }
}

pub(crate) fn switch(source: Operand, case: Case, arms: Vec<(u32, Exec)>) -> Exec {
    Exec::Switch { source, case, arms }
}

pub(crate) fn advance_all(
    state: Operand,
    handler: Operand,
    computation: Operand,
    sequence: Operand,
    extent: Operand,
    context: Operand,
) -> Exec {
    Exec::Advance {
        state,
        handler,
        computation,
        sequence,
        extent,
        context,
    }
}

/// Advance while preserving the current extent and dispatch context.
pub(crate) fn advance_here(computation: Operand, sequence: Operand) -> Exec {
    advance_all(
        state(State::Value),
        state(State::Handler),
        computation,
        sequence,
        state(State::Extent),
        state(State::Context),
    )
}
