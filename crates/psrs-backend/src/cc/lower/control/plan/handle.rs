//! Handle and owned-transfer transitions.

use super::ir::*;
use crate::control::schema::{
    ComputationCase, ContextCase, OwnerCase, SequenceCase, computation as c, owner as o,
    sequence as q,
};

/// Handle normalizes a leading Append, then enters the source under a fresh or
/// shared extent.
pub(super) fn handle(cursor: &mut Cursor) -> Exec {
    let mut ops = Vec::new();
    let ret = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::Handle),
            field: c::HANDLE_RETURN,
        },
    );
    let op = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::Handle),
            field: c::HANDLE_OPERATION,
        },
    );
    let inner = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::Handle),
            field: c::HANDLE_SOURCE,
        },
    );
    let arms = SequenceCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = cursor.clone();
            let arm = match case {
                SequenceCase::Empty | SequenceCase::Cons => {
                    handle_enter_fresh(&mut cursor, ret.clone(), op.clone(), inner.clone())
                }
                SequenceCase::Finish => {
                    handle_enter_finish(&mut cursor, ret.clone(), op.clone(), inner.clone())
                }
                SequenceCase::Append => handle_normalize(&mut cursor),
            };
            (case.tag(), arm)
        })
        .collect();
    steps(
        ops,
        switch(
            state(State::Sequence),
            Case::Sequence(SequenceCase::Empty),
            arms,
        ),
    )
}

pub(super) fn handle_enter_fresh(
    cursor: &mut Cursor,
    ret: Operand,
    op: Operand,
    inner: Operand,
) -> Exec {
    let mut ops = Vec::new();
    let extent = cursor.bind(
        &mut ops,
        Op::FreshExtent {
            parent: Some(state(State::Extent)),
        },
    );
    let context = cursor.bind(
        &mut ops,
        Op::Node {
            case: Case::Context(ContextCase::Scoped),
            fields: vec![
                ret,
                op,
                state(State::Sequence),
                state(State::Extent),
                state(State::Context),
                extent.clone(),
            ],
        },
    );
    let empty = cursor.bind(&mut ops, Op::EmptySequence);
    steps(
        ops,
        advance_all(
            state(State::Value),
            state(State::Handler),
            inner,
            empty,
            extent,
            context,
        ),
    )
}

pub(super) fn handle_enter_finish(
    cursor: &mut Cursor,
    ret: Operand,
    op: Operand,
    inner: Operand,
) -> Exec {
    let mut ops = Vec::new();
    let extent = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Sequence),
            case: Case::Sequence(SequenceCase::Finish),
            field: q::FINISH_EXTENT,
        },
    );
    let saved = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Sequence),
            case: Case::Sequence(SequenceCase::Finish),
            field: q::FINISH_REST,
        },
    );
    let parent = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Sequence),
            case: Case::Sequence(SequenceCase::Finish),
            field: q::FINISH_PARENT,
        },
    );
    let context = cursor.bind(
        &mut ops,
        Op::Node {
            case: Case::Context(ContextCase::Scoped),
            fields: vec![
                ret,
                op,
                saved,
                parent,
                state(State::Context),
                extent.clone(),
            ],
        },
    );
    let empty = cursor.bind(&mut ops, Op::EmptySequence);
    steps(
        ops,
        advance_all(
            state(State::Value),
            state(State::Handler),
            inner,
            empty,
            extent,
            context,
        ),
    )
}

pub(super) fn handle_normalize(cursor: &mut Cursor) -> Exec {
    let mut ops = Vec::new();
    let left = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Sequence),
            case: Case::Sequence(SequenceCase::Append),
            field: q::APPEND_LEFT,
        },
    );
    let right = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Sequence),
            case: Case::Sequence(SequenceCase::Append),
            field: q::APPEND_RIGHT,
        },
    );
    let arms = SequenceCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = cursor.clone();
            let arm = match case {
                SequenceCase::Empty => advance_here(state(State::Computation), right.clone()),
                SequenceCase::Cons | SequenceCase::Append | SequenceCase::Finish => {
                    let (head_field, rest_field, parent_field) = match case {
                        SequenceCase::Cons => (q::CONS_HEAD, q::CONS_TAIL, None),
                        SequenceCase::Append => (q::APPEND_LEFT, q::APPEND_RIGHT, None),
                        SequenceCase::Finish => {
                            (q::FINISH_EXTENT, q::FINISH_REST, Some(q::FINISH_PARENT))
                        }
                        SequenceCase::Empty => unreachable!(),
                    };
                    let mut ops = Vec::new();
                    let head = cursor.bind(
                        &mut ops,
                        Op::Project {
                            source: left.clone(),
                            case: Case::Sequence(*case),
                            field: head_field,
                        },
                    );
                    let rest = cursor.bind(
                        &mut ops,
                        Op::Project {
                            source: left.clone(),
                            case: Case::Sequence(*case),
                            field: rest_field,
                        },
                    );
                    let rest = cursor.bind(
                        &mut ops,
                        Op::Node {
                            case: Case::Sequence(SequenceCase::Append),
                            fields: vec![rest, right.clone()],
                        },
                    );
                    let mut fields = vec![head, rest];
                    if let Some(parent_field) = parent_field {
                        let parent = cursor.bind(
                            &mut ops,
                            Op::Project {
                                source: left.clone(),
                                case: Case::Sequence(*case),
                                field: parent_field,
                            },
                        );
                        fields.push(parent);
                    }
                    let pending = cursor.bind(
                        &mut ops,
                        Op::Node {
                            case: Case::Sequence(*case),
                            fields,
                        },
                    );
                    steps(
                        ops,
                        advance_all(
                            state(State::Value),
                            state(State::Handler),
                            state(State::Computation),
                            pending,
                            state(State::Extent),
                            state(State::Context),
                        ),
                    )
                }
            };
            (case.tag(), arm)
        })
        .collect();
    steps(
        ops,
        switch(left, Case::Sequence(SequenceCase::Append), arms),
    )
}

/// Owned transfer guards Pending, marks Running, loads and clears the callback,
/// applies it, then marks Consumed.
pub(super) fn owned_resume(cursor: &mut Cursor) -> Exec {
    let mut ops = Vec::new();
    let owner = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::OwnedResume),
            field: c::OWNED_OWNER,
        },
    );
    let value = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::OwnedResume),
            field: c::OWNED_VALUE,
        },
    );
    let callback = cursor.bind(
        &mut ops,
        Op::Project {
            source: owner.clone(),
            case: Case::Owner(OwnerCase::OneShot),
            field: o::CALLBACK,
        },
    );
    let status = cursor.bind(
        &mut ops,
        Op::Project {
            source: owner,
            case: Case::Owner(OwnerCase::OneShot),
            field: o::STATUS,
        },
    );
    cursor.bind(
        &mut ops,
        Op::GuardPending {
            cell: status.clone(),
        },
    );
    let base = ops.len();
    let outer = cursor.clone();
    let mut protected = Vec::new();
    cursor.bind(
        &mut protected,
        Op::SetStatus {
            cell: status.clone(),
            value: 1,
        },
    );
    let continuation = cursor.bind(
        &mut protected,
        Op::ReadCallback {
            cell: callback.clone(),
        },
    );
    cursor.bind(
        &mut protected,
        Op::WriteCallback {
            cell: callback,
            value: status.clone(),
        },
    );
    let next = cursor.bind(
        &mut protected,
        Op::Apply {
            callee: continuation,
            args: vec![value],
        },
    );
    cursor.bind(
        &mut protected,
        Op::SetStatus {
            cell: status.clone(),
            value: 2,
        },
    );
    *cursor = outer;
    let next = cursor.bind(
        &mut ops,
        Op::Protect {
            action: crate::control::execution::ExceptionalAction::AcquiredCallbackExit,
            ops: protected,
            result: next,
            cleanup: vec![Op::SetStatus {
                cell: status,
                value: 2,
            }],
        },
    );
    debug_assert_eq!(base + 1, ops.len());
    steps(ops, advance_here(next, state(State::Sequence)))
}
