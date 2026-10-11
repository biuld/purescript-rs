//! Instantiate lifecycle frames when replaying a persistent source checkpoint.
//! Ordinary callbacks and one-shot owner objects are shared, never cloned.

use super::builder::Machine;
use super::executor::arm;
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, AssignmentKind, Function, ValueId};

impl Machine<'_> {
    /// Flatten into a reversed heap sequence with an explicit traversal stack.
    pub(super) fn clone_sequence(mut self) -> Function {
        let queue = self.lower.fresh(erased_shape());
        let todo = self.lower.fresh(erased_shape());
        let reversed = self.lower.fresh(erased_shape());
        let active = self.lower.fresh(erased_shape());
        let parent = self.lower.fresh(erased_shape());
        let mut cases = Vec::new();
        let mut body = Vec::new();
        let mut empty_cases = Vec::new();
        for tag in 0..4 {
            let mut branch = Vec::new();
            let result = if tag == 0 {
                self.replay_call(
                    self.functions.rebuild_sequence,
                    vec![reversed, todo, active, parent],
                    &mut branch,
                )
            } else {
                // The traversal stack is built exclusively from Append nodes.
                if tag == 2 {
                    let next = self.field(todo, self.layout.sequence, 2, 0, &mut branch);
                    let rest = self.field(todo, self.layout.sequence, 2, 1, &mut branch);
                    self.replay_call(
                        self.functions.clone_sequence,
                        vec![next, rest, reversed, active, parent],
                        &mut branch,
                    )
                } else {
                    self.emit(erased_shape(), &mut branch, |_| AssignmentKind::Unreachable)
                }
            };
            empty_cases.push(arm(tag, branch, result));
        }
        let result = self.switch(todo, self.layout.sequence, empty_cases, &mut body);
        cases.push(arm(0, body, result));
        for tag in [1, 2, 3] {
            let mut branch = Vec::new();
            let (next, todo, reversed) = if tag == 2 {
                let left = self.field(queue, self.layout.sequence, 2, 0, &mut branch);
                let right = self.field(queue, self.layout.sequence, 2, 1, &mut branch);
                let todo = self.node(self.layout.sequence, 2, vec![right, todo], &mut branch);
                (left, todo, reversed)
            } else {
                let payload = self.field(queue, self.layout.sequence, tag, 0, &mut branch);
                let rest = self.field(queue, self.layout.sequence, tag, 1, &mut branch);
                let fields = if tag == 1 {
                    vec![payload, reversed]
                } else {
                    self.check_boundary(payload, &mut branch);
                    vec![payload, reversed, parent]
                };
                let reversed = self.node(self.layout.sequence, tag, fields, &mut branch);
                (rest, todo, reversed)
            };
            let result = self.replay_call(
                self.functions.clone_sequence,
                vec![next, todo, reversed, active, parent],
                &mut branch,
            );
            cases.push(arm(tag as i32, branch, result));
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
        let reversed = self.lower.fresh(erased_shape());
        let result_queue = self.lower.fresh(erased_shape());
        let active = self.lower.fresh(erased_shape());
        let parent = self.lower.fresh(erased_shape());
        let mut cases = Vec::new();
        let mut branch = Vec::new();
        let result = self.pair(result_queue, active, &mut branch);
        cases.push(arm(0, branch, result));
        for tag in [1, 3] {
            let mut branch = Vec::new();
            let payload = self.field(reversed, self.layout.sequence, tag, 0, &mut branch);
            let rest = self.field(reversed, self.layout.sequence, tag, 1, &mut branch);
            let (fields, active) = if tag == 1 {
                (vec![payload, result_queue], active)
            } else {
                let extent = self.new_boundary(Some(parent), &mut branch);
                (vec![extent, result_queue, active], extent)
            };
            let next = self.node(self.layout.sequence, tag, fields, &mut branch);
            let result = self.replay_call(
                self.functions.rebuild_sequence,
                vec![rest, next, active, if tag == 3 { active } else { parent }],
                &mut branch,
            );
            cases.push(arm(tag as i32, branch, result));
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
        self.emit(erased_shape(), body, |_| AssignmentKind::DirectCall {
            function,
            arguments,
        })
    }
}
