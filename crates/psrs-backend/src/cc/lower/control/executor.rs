//! A flat driver; only the driver traverses saved continuation sequences.

use super::builder::Machine;
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, Function, TagCase, ValueId};

impl Machine<'_> {
    pub(super) fn executor(mut self, open: bool) -> Function {
        let state = self.lower.fresh(erased_shape());
        let handler = self.lower.fresh(erased_shape());
        let computation = self.lower.fresh(erased_shape());
        let sequence = self.lower.fresh(erased_shape());
        let mut parameters = vec![state, handler, computation, sequence];
        if open {
            let project = self.lower.fresh(erased_shape());
            let inject = self.lower.fresh(erased_shape());
            self.open = Some((project, inject));
            parameters.extend([project, inject]);
        }
        let boundary = self.lower.fresh(erased_shape());
        self.boundary = Some(boundary);
        parameters.push(boundary);
        let mut cases = Vec::new();

        let mut body = Vec::new();
        let value = self.field(computation, self.layout.computation, 0, 0, &mut body);
        let result = self.returned(state, handler, computation, value, sequence, &mut body);
        cases.push(arm(0, body, result));

        let mut body = Vec::new();
        let result = self.requested(state, handler, computation, sequence, &mut body);
        cases.push(arm(1, body, result));

        let mut body = Vec::new();
        let inner = self.field(computation, self.layout.computation, 2, 0, &mut body);
        let continuation = self.field(computation, self.layout.computation, 2, 1, &mut body);
        let pending = self.node(
            self.layout.sequence,
            1,
            vec![continuation, sequence],
            &mut body,
        );
        let result = self.recur(state, handler, inner, pending, &mut body);
        cases.push(arm(2, body, result));

        let mut body = Vec::new();
        let value = self.field(computation, self.layout.computation, 3, 0, &mut body);
        let saved = self.field(computation, self.layout.computation, 3, 1, &mut body);
        let captured_boundary = self.field(computation, self.layout.computation, 3, 2, &mut body);
        self.check_boundary(captured_boundary, &mut body);
        let pending = self.node(self.layout.sequence, 2, vec![saved, sequence], &mut body);
        let next = self.node(self.layout.computation, 0, vec![value], &mut body);
        let result = self.recur(state, handler, next, pending, &mut body);
        cases.push(arm(3, body, result));

        let mut body = Vec::new();
        let result = self.enter_handle(state, handler, computation, sequence, 4, &mut body);
        cases.push(arm(4, body, result));
        let mut body = Vec::new();
        let result = self.enter_handle(state, handler, computation, sequence, 5, &mut body);
        cases.push(arm(5, body, result));

        let mut body = Vec::new();
        let result = self.consume_owned(state, handler, computation, sequence, &mut body);
        cases.push(arm(6, body, result));

        let mut body = Vec::new();
        let result = self.switch(computation, self.layout.computation, cases, &mut body);
        let symbol = if open {
            self.functions.open_executor
        } else {
            self.functions.executor
        };
        self.finish(
            symbol,
            if open {
                "control_open_executor"
            } else {
                "control_executor"
            },
            parameters,
            body,
            result,
        )
    }

    fn returned(
        &mut self,
        state: ValueId,
        handler: ValueId,
        computation: ValueId,
        value: ValueId,
        sequence: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let mut cases = Vec::new();
        let mut branch = Vec::new();
        let pair = self.pair(state, value, &mut branch);
        let result = if self.open.is_some() {
            self.close_boundary(self.boundary.unwrap(), &mut branch);
            self.node(self.layout.computation, 0, vec![pair], &mut branch)
        } else {
            pair
        };
        cases.push(arm(0, branch, result));

        let mut branch = Vec::new();
        let continuation = self.field(sequence, self.layout.sequence, 1, 0, &mut branch);
        let rest = self.field(sequence, self.layout.sequence, 1, 1, &mut branch);
        let next = self.apply(continuation, &[value], &mut branch);
        let result = self.recur(state, handler, next, rest, &mut branch);
        cases.push(arm(1, branch, result));

        let mut branch = Vec::new();
        let left = self.field(sequence, self.layout.sequence, 2, 0, &mut branch);
        let right = self.field(sequence, self.layout.sequence, 2, 1, &mut branch);
        let result = self.appended(state, handler, computation, value, left, right, &mut branch);
        cases.push(arm(2, branch, result));
        self.switch(sequence, self.layout.sequence, cases, body)
    }

    #[allow(clippy::too_many_arguments)]
    fn appended(
        &mut self,
        state: ValueId,
        handler: ValueId,
        computation: ValueId,
        value: ValueId,
        left: ValueId,
        right: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let mut cases = Vec::new();
        let mut branch = Vec::new();
        let result = self.recur(state, handler, computation, right, &mut branch);
        cases.push(arm(0, branch, result));

        let mut branch = Vec::new();
        let continuation = self.field(left, self.layout.sequence, 1, 0, &mut branch);
        let rest = self.field(left, self.layout.sequence, 1, 1, &mut branch);
        let pending = self.node(self.layout.sequence, 2, vec![rest, right], &mut branch);
        let next = self.apply(continuation, &[value], &mut branch);
        let result = self.recur(state, handler, next, pending, &mut branch);
        cases.push(arm(1, branch, result));

        let mut branch = Vec::new();
        let first = self.field(left, self.layout.sequence, 2, 0, &mut branch);
        let second = self.field(left, self.layout.sequence, 2, 1, &mut branch);
        let rest = self.node(self.layout.sequence, 2, vec![second, right], &mut branch);
        let pending = self.node(self.layout.sequence, 2, vec![first, rest], &mut branch);
        let result = self.recur(state, handler, computation, pending, &mut branch);
        cases.push(arm(2, branch, result));
        self.switch(left, self.layout.sequence, cases, body)
    }
}

pub(super) fn arm(tag: i32, assignments: Vec<Assignment>, value: ValueId) -> TagCase {
    TagCase {
        tag,
        assignments,
        value,
    }
}
