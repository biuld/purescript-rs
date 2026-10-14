//! Checked plans for the machine's helper bodies: replay, lifecycle and the
//! one-shot factories. Each builder names its parameters positionally; the
//! expander binds them to the function's fresh parameter values.

use super::ir::{Case, Cursor, Exec, Op, Operand, Product, Shape, steps, switch};
use crate::cc::ControlHelperRole;
use crate::control::schema::{
    ComputationCase, ExtentCase, OwnerCase, SequenceCase, extent as e, owner as o, sequence as q,
};

fn param(index: usize) -> Operand {
    Operand::Param(index)
}

fn project(source: Operand, case: Case, field: u32) -> Op {
    Op::Project {
        source,
        case,
        field,
    }
}

fn tail(role: ControlHelperRole, args: Vec<Operand>) -> Exec {
    Exec::Tail { role, args }
}

/// Flatten the current queue into a reversed sequence with an explicit stack.
///
/// Parameters: `queue`, `todo`, `reversed`, `active`, `parent`.
pub(crate) fn clone_sequence() -> Exec {
    let cursor = Cursor::new();
    let arms = SequenceCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = cursor.clone();
            let arm = match case {
                SequenceCase::Empty => clone_todo(&cursor),
                SequenceCase::Cons => {
                    let mut ops = Vec::new();
                    let head = cursor.bind(
                        &mut ops,
                        project(param(0), Case::Sequence(SequenceCase::Cons), q::CONS_HEAD),
                    );
                    let rest = cursor.bind(
                        &mut ops,
                        project(param(0), Case::Sequence(SequenceCase::Cons), q::CONS_TAIL),
                    );
                    let reversed = cursor.bind(
                        &mut ops,
                        Op::Node {
                            case: Case::Sequence(SequenceCase::Cons),
                            fields: vec![head, param(2)],
                        },
                    );
                    steps(
                        ops,
                        tail(
                            ControlHelperRole::CloneSequence,
                            vec![rest, param(1), reversed, param(3), param(4)],
                        ),
                    )
                }
                SequenceCase::Append => {
                    let mut ops = Vec::new();
                    let left = cursor.bind(
                        &mut ops,
                        project(
                            param(0),
                            Case::Sequence(SequenceCase::Append),
                            q::APPEND_LEFT,
                        ),
                    );
                    let right = cursor.bind(
                        &mut ops,
                        project(
                            param(0),
                            Case::Sequence(SequenceCase::Append),
                            q::APPEND_RIGHT,
                        ),
                    );
                    let todo = cursor.bind(
                        &mut ops,
                        Op::Node {
                            case: Case::Sequence(SequenceCase::Append),
                            fields: vec![right, param(1)],
                        },
                    );
                    steps(
                        ops,
                        tail(
                            ControlHelperRole::CloneSequence,
                            vec![left, todo, param(2), param(3), param(4)],
                        ),
                    )
                }
                SequenceCase::Finish => {
                    let mut ops = Vec::new();
                    let extent = cursor.bind(
                        &mut ops,
                        project(
                            param(0),
                            Case::Sequence(SequenceCase::Finish),
                            q::FINISH_EXTENT,
                        ),
                    );
                    let rest = cursor.bind(
                        &mut ops,
                        project(
                            param(0),
                            Case::Sequence(SequenceCase::Finish),
                            q::FINISH_REST,
                        ),
                    );
                    cursor.bind(
                        &mut ops,
                        Op::CheckBoundary {
                            extent: extent.clone(),
                        },
                    );
                    let reversed = cursor.bind(
                        &mut ops,
                        Op::Node {
                            case: Case::Sequence(SequenceCase::Finish),
                            fields: vec![extent, param(2), param(4)],
                        },
                    );
                    steps(
                        ops,
                        tail(
                            ControlHelperRole::CloneSequence,
                            vec![rest, param(1), reversed, param(3), param(4)],
                        ),
                    )
                }
            };
            (case.tag(), arm)
        })
        .collect();
    switch(param(0), Case::Sequence(SequenceCase::Empty), arms)
}

/// The traversal stack is built only from Append nodes; any other pending node
/// is a rejection, not a silent fallthrough.
fn clone_todo(cursor: &Cursor) -> Exec {
    let arms = SequenceCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = cursor.clone();
            let arm = match case {
                SequenceCase::Empty => tail(
                    ControlHelperRole::RebuildSequence,
                    vec![param(2), param(1), param(3), param(4)],
                ),
                SequenceCase::Append => {
                    let mut ops = Vec::new();
                    let next = cursor.bind(
                        &mut ops,
                        project(
                            param(1),
                            Case::Sequence(SequenceCase::Append),
                            q::APPEND_LEFT,
                        ),
                    );
                    let rest = cursor.bind(
                        &mut ops,
                        project(
                            param(1),
                            Case::Sequence(SequenceCase::Append),
                            q::APPEND_RIGHT,
                        ),
                    );
                    steps(
                        ops,
                        tail(
                            ControlHelperRole::CloneSequence,
                            vec![next, rest, param(2), param(3), param(4)],
                        ),
                    )
                }
                SequenceCase::Cons | SequenceCase::Finish => Exec::Reject {
                    shape: Shape::ReplayResult,
                },
            };
            (case.tag(), arm)
        })
        .collect();
    switch(param(1), Case::Sequence(SequenceCase::Empty), arms)
}

/// Rebuild the flattened queue, giving every Finish a fresh child extent.
///
/// Parameters: `reversed`, `result_queue`, `active`, `parent`.
pub(crate) fn rebuild_sequence() -> Exec {
    let cursor = Cursor::new();
    let arms = [
        SequenceCase::Empty,
        SequenceCase::Cons,
        SequenceCase::Finish,
    ]
    .iter()
    .map(|case| {
        let mut cursor = cursor.clone();
        let arm = match case {
            SequenceCase::Empty => {
                let mut ops = Vec::new();
                let result = cursor.bind(
                    &mut ops,
                    Op::Product {
                        product: Product::ReplayResult,
                        fields: vec![param(1), param(2)],
                    },
                );
                steps(ops, Exec::Return { value: result })
            }
            SequenceCase::Cons => {
                let mut ops = Vec::new();
                let head = cursor.bind(
                    &mut ops,
                    project(param(0), Case::Sequence(SequenceCase::Cons), q::CONS_HEAD),
                );
                let rest = cursor.bind(
                    &mut ops,
                    project(param(0), Case::Sequence(SequenceCase::Cons), q::CONS_TAIL),
                );
                let next = cursor.bind(
                    &mut ops,
                    Op::Node {
                        case: Case::Sequence(SequenceCase::Cons),
                        fields: vec![head, param(1)],
                    },
                );
                steps(
                    ops,
                    tail(
                        ControlHelperRole::RebuildSequence,
                        vec![rest, next, param(2), param(3)],
                    ),
                )
            }
            SequenceCase::Append => Exec::Reject {
                shape: Shape::ReplayResult,
            },
            SequenceCase::Finish => {
                let mut ops = Vec::new();
                // The saved finish's extent is inspected but replaced by a
                // fresh activation extent, matching the replay contract.
                let _saved = cursor.bind(
                    &mut ops,
                    project(
                        param(0),
                        Case::Sequence(SequenceCase::Finish),
                        q::FINISH_EXTENT,
                    ),
                );
                let rest = cursor.bind(
                    &mut ops,
                    project(
                        param(0),
                        Case::Sequence(SequenceCase::Finish),
                        q::FINISH_REST,
                    ),
                );
                let extent = cursor.bind(
                    &mut ops,
                    Op::FreshExtent {
                        parent: Some(param(3)),
                    },
                );
                let next = cursor.bind(
                    &mut ops,
                    Op::Node {
                        case: Case::Sequence(SequenceCase::Finish),
                        fields: vec![extent.clone(), param(1), param(2)],
                    },
                );
                steps(
                    ops,
                    tail(
                        ControlHelperRole::RebuildSequence,
                        vec![rest, next, extent.clone(), extent],
                    ),
                )
            }
        };
        (case.tag(), arm)
    })
    .collect();
    switch(param(0), Case::Sequence(SequenceCase::Empty), arms)
}

/// Guard the current extent's own status and continue with its parent.
///
/// Parameters: `boundary`.
pub(crate) fn boundary_check() -> Exec {
    let arms = ExtentCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = Cursor::new();
            let mut ops = Vec::new();
            let cell = cursor.bind(&mut ops, project(param(0), Case::Extent(*case), e::STATUS));
            cursor.bind(&mut ops, Op::GuardPending { cell });
            let arm = match case {
                ExtentCase::Root => Exec::Return { value: param(0) },
                ExtentCase::Scoped => {
                    let parent =
                        cursor.bind(&mut ops, project(param(0), Case::Extent(*case), e::PARENT));
                    tail(ControlHelperRole::Check, vec![parent])
                }
            };
            (case.tag(), steps(ops, arm))
        })
        .collect();
    switch(param(0), Case::Extent(ExtentCase::Root), arms)
}

/// Close the current extent's own status and return it.
///
/// Parameters: `boundary`.
pub(crate) fn boundary_close() -> Exec {
    let arms = ExtentCase::ALL
        .iter()
        .map(|case| {
            let mut cursor = Cursor::new();
            let mut ops = Vec::new();
            let cell = cursor.bind(&mut ops, project(param(0), Case::Extent(*case), e::STATUS));
            cursor.bind(&mut ops, Op::SetStatus { cell, value: 1 });
            (case.tag(), steps(ops, Exec::Return { value: param(0) }))
        })
        .collect();
    switch(param(0), Case::Extent(ExtentCase::Root), arms)
}

/// Allocate a fresh root extent.
pub(crate) fn start_boundary() -> Exec {
    let mut cursor = Cursor::new();
    let mut ops = Vec::new();
    let extent = cursor.bind(&mut ops, Op::FreshExtent { parent: None });
    steps(ops, Exec::Return { value: extent })
}

/// Allocate a one-shot owner object.
///
/// Parameters: `continuation`.
pub(crate) fn once() -> Exec {
    let mut cursor = Cursor::new();
    let mut ops = Vec::new();
    let callback = cursor.bind(&mut ops, Op::NewCallbackCell { value: param(0) });
    let status = cursor.bind(&mut ops, Op::NewStatus);
    let owner = cursor.bind(
        &mut ops,
        Op::Node {
            case: Case::Owner(OwnerCase::OneShot),
            fields: vec![callback, status],
        },
    );
    steps(ops, Exec::Return { value: owner })
}

/// Guard Pending, mark Cancelled, clear the callback and return the owner.
///
/// Parameters: `owner`.
pub(crate) fn cancel() -> Exec {
    let mut cursor = Cursor::new();
    let mut ops = Vec::new();
    let status = cursor.bind(
        &mut ops,
        project(param(0), Case::Owner(OwnerCase::OneShot), o::STATUS),
    );
    cursor.bind(
        &mut ops,
        Op::GuardPending {
            cell: status.clone(),
        },
    );
    cursor.bind(
        &mut ops,
        Op::SetStatus {
            cell: status.clone(),
            value: 3,
        },
    );
    let callback = cursor.bind(
        &mut ops,
        project(param(0), Case::Owner(OwnerCase::OneShot), o::CALLBACK),
    );
    cursor.bind(
        &mut ops,
        Op::WriteCallback {
            cell: callback,
            value: status,
        },
    );
    steps(ops, Exec::Return { value: param(0) })
}

/// The raw-resume closure adapter: read its saved queue and checkpoint
/// captures, check the checkpoint, and enter the raw `Resume` frame.
///
/// Parameters: `receiver`, `value`.
pub(crate) fn raw_resume() -> Exec {
    let mut cursor = Cursor::new();
    let mut ops = Vec::new();
    let saved = cursor.bind(
        &mut ops,
        Op::Capture {
            closure: param(0),
            index: 0,
            case: Case::Sequence(SequenceCase::Empty),
        },
    );
    let boundary = cursor.bind(
        &mut ops,
        Op::Capture {
            closure: param(0),
            index: 1,
            case: Case::Extent(ExtentCase::Root),
        },
    );
    cursor.bind(
        &mut ops,
        Op::CheckBoundary {
            extent: boundary.clone(),
        },
    );
    let result = cursor.bind(
        &mut ops,
        Op::Node {
            case: Case::Computation(ComputationCase::Resume),
            fields: vec![param(1), saved, boundary],
        },
    );
    steps(ops, Exec::Return { value: result })
}
