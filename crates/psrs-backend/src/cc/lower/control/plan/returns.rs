//! Return/queue transitions of the flat driver.

use super::ir::*;
use crate::control::schema::{
    ComputationCase, ContextCase, SequenceCase, computation as c, context as x, sequence as q,
};

/// Return publishes or applies the current return clause, then consumes the
/// current queue.
pub(super) fn returned(cursor: &mut Cursor) -> Exec {
    let mut ops = Vec::new();
    let value = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Computation),
            case: Case::Computation(ComputationCase::Return),
            field: c::PAYLOAD,
        },
    );
    let arms = SequenceCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = cursor.clone();
            let arm = match case {
                SequenceCase::Empty => return_empty(&cursor, value.clone()),
                SequenceCase::Cons => return_cons(&mut cursor, value.clone()),
                SequenceCase::Append => return_append(&mut cursor, value.clone()),
                SequenceCase::Finish => return_finish(&mut cursor),
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

/// A return at an empty queue publishes at the root or enters a scoped target.
pub(super) fn return_empty(cursor: &Cursor, value: Operand) -> Exec {
    let arms = ContextCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = cursor.clone();
            let arm = match case {
                ContextCase::Root => {
                    let mut ops = Vec::new();
                    let pair = cursor.bind(
                        &mut ops,
                        Op::Pair {
                            state: state(State::Value),
                            value: value.clone(),
                        },
                    );
                    steps(ops, Exec::Publish { result: pair })
                }
                ContextCase::Scoped => return_clause(&mut cursor, value.clone()),
            };
            (case.tag(), arm)
        })
        .collect();
    switch(
        state(State::Context),
        Case::Context(ContextCase::Root),
        arms,
    )
}

/// Apply the scoped return clause and enter the target with a fresh finish.
pub(super) fn return_clause(cursor: &mut Cursor, value: Operand) -> Exec {
    let mut ops = Vec::new();
    let ret = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Context),
            case: Case::Context(ContextCase::Scoped),
            field: x::SCOPED_RETURN,
        },
    );
    let target = cursor.bind(
        &mut ops,
        Op::Apply {
            callee: ret,
            args: vec![value],
        },
    );
    steps(ops, enter_target(cursor, target))
}

/// Build the finish record for the current scoped context and advance into the
/// target under the caller's context and the shared extent.
pub(super) fn enter_target(cursor: &mut Cursor, target: Operand) -> Exec {
    let mut ops = Vec::new();
    let saved = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Context),
            case: Case::Context(ContextCase::Scoped),
            field: x::SCOPED_SAVED,
        },
    );
    let parent = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Context),
            case: Case::Context(ContextCase::Scoped),
            field: x::SCOPED_PARENT,
        },
    );
    let caller = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Context),
            case: Case::Context(ContextCase::Scoped),
            field: x::SCOPED_CALLER,
        },
    );
    let extent = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Context),
            case: Case::Context(ContextCase::Scoped),
            field: x::SCOPED_EXTENT,
        },
    );
    let finish = cursor.bind(
        &mut ops,
        Op::Node {
            case: Case::Sequence(SequenceCase::Finish),
            fields: vec![extent.clone(), saved, parent],
        },
    );
    steps(
        ops,
        advance_all(
            state(State::Value),
            state(State::Handler),
            target,
            finish,
            extent,
            caller,
        ),
    )
}

pub(super) fn return_cons(cursor: &mut Cursor, value: Operand) -> Exec {
    let mut ops = Vec::new();
    let head = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Sequence),
            case: Case::Sequence(SequenceCase::Cons),
            field: q::CONS_HEAD,
        },
    );
    let tail = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Sequence),
            case: Case::Sequence(SequenceCase::Cons),
            field: q::CONS_TAIL,
        },
    );
    let next = cursor.bind(
        &mut ops,
        Op::Apply {
            callee: head,
            args: vec![value],
        },
    );
    steps(ops, advance_here(next, tail))
}

pub(super) fn return_append(cursor: &mut Cursor, value: Operand) -> Exec {
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
    steps(ops, append(cursor, value, left, right))
}

/// Reassociate an Append so left-to-right work and Finish ownership are kept.
pub(super) fn append(cursor: &Cursor, value: Operand, left: Operand, right: Operand) -> Exec {
    let arms = SequenceCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = cursor.clone();
            let arm = match case {
                SequenceCase::Empty => advance_here(state(State::Computation), right.clone()),
                SequenceCase::Cons => {
                    let mut ops = Vec::new();
                    let head = cursor.bind(
                        &mut ops,
                        Op::Project {
                            source: left.clone(),
                            case: Case::Sequence(SequenceCase::Cons),
                            field: q::CONS_HEAD,
                        },
                    );
                    let rest = cursor.bind(
                        &mut ops,
                        Op::Project {
                            source: left.clone(),
                            case: Case::Sequence(SequenceCase::Cons),
                            field: q::CONS_TAIL,
                        },
                    );
                    let pending = cursor.bind(
                        &mut ops,
                        Op::Node {
                            case: Case::Sequence(SequenceCase::Append),
                            fields: vec![rest, right.clone()],
                        },
                    );
                    let next = cursor.bind(
                        &mut ops,
                        Op::Apply {
                            callee: head,
                            args: vec![value.clone()],
                        },
                    );
                    steps(ops, advance_here(next, pending))
                }
                SequenceCase::Append => {
                    let mut ops = Vec::new();
                    let first = cursor.bind(
                        &mut ops,
                        Op::Project {
                            source: left.clone(),
                            case: Case::Sequence(SequenceCase::Append),
                            field: q::APPEND_LEFT,
                        },
                    );
                    let second = cursor.bind(
                        &mut ops,
                        Op::Project {
                            source: left.clone(),
                            case: Case::Sequence(SequenceCase::Append),
                            field: q::APPEND_RIGHT,
                        },
                    );
                    let rest = cursor.bind(
                        &mut ops,
                        Op::Node {
                            case: Case::Sequence(SequenceCase::Append),
                            fields: vec![second, right.clone()],
                        },
                    );
                    let pending = cursor.bind(
                        &mut ops,
                        Op::Node {
                            case: Case::Sequence(SequenceCase::Append),
                            fields: vec![first, rest],
                        },
                    );
                    steps(ops, advance_here(state(State::Computation), pending))
                }
                SequenceCase::Finish => {
                    let mut ops = Vec::new();
                    let extent = cursor.bind(
                        &mut ops,
                        Op::Project {
                            source: left.clone(),
                            case: Case::Sequence(SequenceCase::Finish),
                            field: q::FINISH_EXTENT,
                        },
                    );
                    let rest = cursor.bind(
                        &mut ops,
                        Op::Project {
                            source: left.clone(),
                            case: Case::Sequence(SequenceCase::Finish),
                            field: q::FINISH_REST,
                        },
                    );
                    let parent = cursor.bind(
                        &mut ops,
                        Op::Project {
                            source: left.clone(),
                            case: Case::Sequence(SequenceCase::Finish),
                            field: q::FINISH_PARENT,
                        },
                    );
                    let pending = cursor.bind(
                        &mut ops,
                        Op::Node {
                            case: Case::Sequence(SequenceCase::Append),
                            fields: vec![rest, right.clone()],
                        },
                    );
                    let finish = cursor.bind(
                        &mut ops,
                        Op::Node {
                            case: Case::Sequence(SequenceCase::Finish),
                            fields: vec![extent, pending, parent],
                        },
                    );
                    steps(ops, advance_here(state(State::Computation), finish))
                }
            };
            (case.tag(), arm)
        })
        .collect();
    switch(left, Case::Sequence(SequenceCase::Append), arms)
}

pub(super) fn return_finish(cursor: &mut Cursor) -> Exec {
    let mut ops = Vec::new();
    let extent = cursor.bind(
        &mut ops,
        Op::Project {
            source: state(State::Sequence),
            case: Case::Sequence(SequenceCase::Finish),
            field: q::FINISH_EXTENT,
        },
    );
    let rest = cursor.bind(
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
    cursor.bind(&mut ops, Op::CloseBoundary { extent });
    steps(
        ops,
        advance_all(
            state(State::Value),
            state(State::Handler),
            state(State::Computation),
            rest,
            parent,
            state(State::Context),
        ),
    )
}
