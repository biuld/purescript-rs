//! Instantiate lifecycle frames when replaying a persistent source checkpoint.
//! Ordinary callbacks and one-shot owner objects are shared, never cloned.

use super::builder::{Machine, repr};
use super::executor::arm;
use crate::cc::{Assignment, AssignmentKind, Function, ValueId};
use crate::control::schema::{SequenceCase, sequence as q};

impl Machine<'_> {
    /// Flatten into a reversed heap sequence with an explicit traversal stack.
    pub(super) fn clone_sequence(mut self) -> Function {
        let queue = self.lower.fresh(repr(self.layout.sequence));
        let todo = self.lower.fresh(repr(self.layout.sequence));
        let reversed = self.lower.fresh(repr(self.layout.sequence));
        let active = self.lower.fresh(repr(self.layout.boundary));
        let parent = self.lower.fresh(repr(self.layout.boundary));
        let mut cases = Vec::new();
        let mut body = Vec::new();
        let mut empty_cases = Vec::new();
        for case in SequenceCase::ALL {
            let mut branch = Vec::new();
            let result = if case == SequenceCase::Empty {
                self.replay_call(
                    self.functions.rebuild_sequence,
                    vec![reversed, todo, active, parent],
                    &mut branch,
                )
            } else if case == SequenceCase::Append {
                // The traversal stack is built exclusively from Append nodes.
                let next = self.field(
                    todo,
                    self.layout.sequence,
                    case.tag(),
                    q::APPEND_LEFT,
                    &mut branch,
                );
                let rest = self.field(
                    todo,
                    self.layout.sequence,
                    case.tag(),
                    q::APPEND_RIGHT,
                    &mut branch,
                );
                self.replay_call(
                    self.functions.clone_sequence,
                    vec![next, rest, reversed, active, parent],
                    &mut branch,
                )
            } else {
                self.emit(repr(self.layout.replay_result), &mut branch, |_| {
                    AssignmentKind::Unreachable
                })
            };
            empty_cases.push(arm(case.tag(), branch, result));
        }
        let result = self.switch(todo, self.layout.sequence, empty_cases, &mut body);
        cases.push(arm(SequenceCase::Empty.tag(), body, result));
        for case in [
            SequenceCase::Cons,
            SequenceCase::Append,
            SequenceCase::Finish,
        ] {
            let mut branch = Vec::new();
            let (next, todo, reversed) = if case == SequenceCase::Append {
                let left = self.field(
                    queue,
                    self.layout.sequence,
                    case.tag(),
                    q::APPEND_LEFT,
                    &mut branch,
                );
                let right = self.field(
                    queue,
                    self.layout.sequence,
                    case.tag(),
                    q::APPEND_RIGHT,
                    &mut branch,
                );
                let todo = self.node(
                    self.layout.sequence,
                    SequenceCase::Append.tag(),
                    vec![right, todo],
                    &mut branch,
                );
                (left, todo, reversed)
            } else {
                let payload = self.field(
                    queue,
                    self.layout.sequence,
                    case.tag(),
                    q::CONS_HEAD,
                    &mut branch,
                );
                let rest = self.field(
                    queue,
                    self.layout.sequence,
                    case.tag(),
                    q::CONS_TAIL,
                    &mut branch,
                );
                let fields = if case == SequenceCase::Cons {
                    vec![payload, reversed]
                } else {
                    self.check_boundary(payload, &mut branch);
                    vec![payload, reversed, parent]
                };
                let reversed = self.node(self.layout.sequence, case.tag(), fields, &mut branch);
                (rest, todo, reversed)
            };
            let result = self.replay_call(
                self.functions.clone_sequence,
                vec![next, todo, reversed, active, parent],
                &mut branch,
            );
            cases.push(arm(case.tag(), branch, result));
        }
        let mut body = Vec::new();
        let result = self.switch(queue, self.layout.sequence, cases, &mut body);
        let symbol = self.functions.clone_sequence;
        self.finish(
            symbol,
            "control_clone_sequence",
            vec![queue, todo, reversed, active, parent],
            body,
            result,
        )
    }

    /// Rebuild from the outside inward, giving each finish a fresh child extent.
    pub(super) fn rebuild_sequence(mut self) -> Function {
        let reversed = self.lower.fresh(repr(self.layout.sequence));
        let result_queue = self.lower.fresh(repr(self.layout.sequence));
        let active = self.lower.fresh(repr(self.layout.boundary));
        let parent = self.lower.fresh(repr(self.layout.boundary));
        let mut cases = Vec::new();
        let mut branch = Vec::new();
        let result = self.replay_result(result_queue, active, &mut branch);
        cases.push(arm(SequenceCase::Empty.tag(), branch, result));
        for case in [SequenceCase::Cons, SequenceCase::Finish] {
            let mut branch = Vec::new();
            let payload = self.field(
                reversed,
                self.layout.sequence,
                case.tag(),
                q::CONS_HEAD,
                &mut branch,
            );
            let rest = self.field(
                reversed,
                self.layout.sequence,
                case.tag(),
                q::CONS_TAIL,
                &mut branch,
            );
            let (fields, active) = if case == SequenceCase::Cons {
                (vec![payload, result_queue], active)
            } else {
                let extent = self.new_boundary(Some(parent), &mut branch);
                (vec![extent, result_queue, active], extent)
            };
            let next = self.node(self.layout.sequence, case.tag(), fields, &mut branch);
            let result = self.replay_call(
                self.functions.rebuild_sequence,
                vec![
                    rest,
                    next,
                    active,
                    if case == SequenceCase::Finish {
                        active
                    } else {
                        parent
                    },
                ],
                &mut branch,
            );
            cases.push(arm(case.tag(), branch, result));
        }
        let mut body = Vec::new();
        let result = self.switch(reversed, self.layout.sequence, cases, &mut body);
        let symbol = self.functions.rebuild_sequence;
        self.finish(
            symbol,
            "control_rebuild_sequence",
            vec![reversed, result_queue, active, parent],
            body,
            result,
        )
    }

    fn replay_call(
        &mut self,
        function: psrs_hir::SymbolId,
        arguments: Vec<ValueId>,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        self.call(function, arguments, body)
    }

    fn replay_result(
        &mut self,
        queue: ValueId,
        active: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let representation = self.layout.replay_result;
        self.emit(repr(representation), body, |destination| {
            AssignmentKind::ProductNew {
                destination,
                representation,
                arguments: vec![queue, active],
            }
        })
    }

    pub(super) fn replay_field(
        &mut self,
        value: ValueId,
        field: u32,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let representation = self.layout.replay_result;
        let Some(crate::cc::Representation::Product { fields }) =
            self.lower.representations.representation(representation)
        else {
            unreachable!("predeclared replay result product");
        };
        let shape = *fields
            .get(field as usize)
            .expect("predeclared replay result field");
        self.emit(shape, body, |destination| AssignmentKind::ProductGet {
            destination,
            representation,
            field,
            value,
        })
    }
}
