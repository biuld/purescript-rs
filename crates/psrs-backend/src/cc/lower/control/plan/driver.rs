//! Perform, Bind and raw Resume transitions.

use super::ir::*;
use super::returns::enter_target;
use crate::cc::ControlHelperRole;
use crate::control::schema::{
    ComputationCase, ContextCase, SequenceCase, computation as c, context as x, replay, result_pair,
};

/// Perform applies the current dispatch clause with a fresh raw resumption.
pub(super) fn perform(cursor: &mut Cursor) -> Exec {
    let mut ops = Vec::new();
    let request = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::Perform),
            field: c::PAYLOAD,
        },
    );
    let arms = ContextCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = cursor.clone();
            let arm = match case {
                ContextCase::Root => perform_root(&mut cursor, request.clone()),
                ContextCase::Scoped => perform_scoped(&mut cursor, request.clone()),
            };
            (case.tag(), arm)
        })
        .collect();
    steps(
        ops,
        switch(
            state(State::Context),
            Case::Context(ContextCase::Root),
            arms,
        ),
    )
}

pub(super) fn perform_root(cursor: &mut Cursor, request: Operand) -> Exec {
    let mut ops = Vec::new();
    let extent = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Context),
            case: Case::Context(ContextCase::Root),
            field: x::ROOT_EXTENT,
        },
    );
    let continuation = cursor.bind(
        &mut ops,
        Op::ResumeClosure {
            captures: vec![state(State::Sequence), extent],
        },
    );
    let pair = cursor.bind(
        &mut ops,
        Op::Apply {
            callee: state(State::Handler),
            args: vec![state(State::Value), request, continuation],
        },
    );
    let next_state = cursor.bind(
        &mut ops,
        Op::PairField {
            pair: pair.clone(),
            field: result_pair::STATE,
        },
    );
    let next = cursor.bind(
        &mut ops,
        Op::PairField {
            pair,
            field: result_pair::VALUE,
        },
    );
    let empty = cursor.bind(&mut ops, Op::EmptySequence);
    steps(
        ops,
        advance_all(
            next_state,
            state(State::Handler),
            next,
            empty,
            state(State::Extent),
            state(State::Context),
        ),
    )
}

pub(super) fn perform_scoped(cursor: &mut Cursor, request: Operand) -> Exec {
    let mut ops = Vec::new();
    let extent = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Context),
            case: Case::Context(ContextCase::Scoped),
            field: x::SCOPED_EXTENT,
        },
    );
    let continuation = cursor.bind(
        &mut ops,
        Op::ResumeClosure {
            captures: vec![state(State::Sequence), extent],
        },
    );
    let clause = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Context),
            case: Case::Context(ContextCase::Scoped),
            field: x::SCOPED_OPERATION,
        },
    );
    let target = cursor.bind(
        &mut ops,
        Op::Apply {
            callee: clause,
            args: vec![request, continuation],
        },
    );
    steps(ops, enter_target(cursor, target))
}

pub(super) fn bind(cursor: &mut Cursor) -> Exec {
    let mut ops = Vec::new();
    let inner = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::Bind),
            field: c::BIND_INNER,
        },
    );
    let continuation = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::Bind),
            field: c::BIND_CONTINUATION,
        },
    );
    let pending = cursor.bind(
        &mut ops,
        Op::Node {
            case: Case::Sequence(SequenceCase::Cons),
            fields: vec![continuation, state(State::Sequence)],
        },
    );
    steps(ops, advance_here(inner, pending))
}

/// Raw resume checks its checkpoint and replays the saved sequence before the
/// consumer queue.
pub(super) fn resume(cursor: &mut Cursor) -> Exec {
    let mut ops = Vec::new();
    let value = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::Resume),
            field: c::RESUME_VALUE,
        },
    );
    let saved = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::Resume),
            field: c::RESUME_SAVED,
        },
    );
    let checkpoint = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::Resume),
            field: c::RESUME_CHECKPOINT,
        },
    );
    cursor.bind(&mut ops, Op::CheckBoundary { extent: checkpoint });
    let empty = cursor.bind(&mut ops, Op::EmptySequence);
    let parent = cursor.bind(
        &mut ops,
        Op::ContextExtent {
            context: state(State::Context),
        },
    );
    let cloned = cursor.bind(
        &mut ops,
        Op::Helper {
            role: ControlHelperRole::CloneSequence,
            args: vec![saved, empty.clone(), empty, state(State::Extent), parent],
        },
    );
    let replay_queue = cursor.bind(
        &mut ops,
        Op::ReplayField {
            result: cloned.clone(),
            field: replay::QUEUE,
        },
    );
    let active = cursor.bind(
        &mut ops,
        Op::ReplayField {
            result: cloned,
            field: replay::ACTIVE,
        },
    );
    let pending = cursor.bind(
        &mut ops,
        Op::Node {
            case: Case::Sequence(SequenceCase::Append),
            fields: vec![replay_queue, state(State::Sequence)],
        },
    );
    let next = cursor.bind(
        &mut ops,
        Op::Node {
            case: Case::Computation(ComputationCase::Return),
            fields: vec![value],
        },
    );
    steps(
        ops,
        advance_all(
            state(State::Value),
            state(State::Handler),
            next,
            pending,
            active,
            state(State::Context),
        ),
    )
}
