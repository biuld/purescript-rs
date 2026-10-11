//! Source dispatch and target lifetime are separate heap contexts.

use super::builder::Machine;
use super::executor::arm;
use crate::cc::{Assignment, ValueId};

impl Machine<'_> {
    pub(super) fn dispatch_boundary(&mut self, body: &mut Vec<Assignment>) -> ValueId {
        let context = self.context.unwrap();
        let mut cases = Vec::new();
        for tag in 0..2 {
            let mut branch = Vec::new();
            let index = if tag == 0 { 0 } else { 5 };
            let extent = self.field(context, self.layout.context, tag, index, &mut branch);
            cases.push(arm(tag as i32, branch, extent));
        }
        self.switch(context, self.layout.context, cases, body)
    }

    pub(super) fn enter_handle(
        &mut self,
        state: ValueId,
        handler: ValueId,
        computation: ValueId,
        sequence: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let ret = self.field(computation, self.layout.computation, 4, 0, body);
        let op = self.field(computation, self.layout.computation, 4, 1, body);
        let inner = self.field(computation, self.layout.computation, 4, 2, body);
        let caller = self.context.unwrap();
        let parent = self.boundary.unwrap();
        let mut cases = Vec::new();
        for tag in 0..4 {
            let mut branch = Vec::new();
            if tag == 2 {
                let result =
                    self.normalize_handle_queue(state, handler, computation, sequence, &mut branch);
                cases.push(arm(tag, branch, result));
                continue;
            }
            // A leading compiler finish marker has the same checked target
            // answer boundary as this tail-returned Handle. Share that extent
            // and remove its duplicate finish; no user continuation is skipped.
            let (saved, parent, extent) = if tag == 3 {
                let extent = self.field(sequence, self.layout.sequence, 3, 0, &mut branch);
                let saved = self.field(sequence, self.layout.sequence, 3, 1, &mut branch);
                let parent = self.field(sequence, self.layout.sequence, 3, 2, &mut branch);
                (saved, parent, extent)
            } else {
                let extent = self.new_boundary(Some(parent), &mut branch);
                (sequence, parent, extent)
            };
            let context = self.node(
                self.layout.context,
                1,
                vec![ret, op, saved, parent, caller, extent],
                &mut branch,
            );
            let empty = self.node(self.layout.sequence, 0, vec![], &mut branch);
            let old_context = self.context.replace(context);
            let old_boundary = self.boundary.replace(extent);
            let result = self.recur(state, handler, inner, empty, &mut branch);
            self.context = old_context;
            self.boundary = old_boundary;
            cases.push(arm(tag, branch, result));
        }
        self.switch(sequence, self.layout.sequence, cases, body)
    }

    fn normalize_handle_queue(
        &mut self,
        state: ValueId,
        handler: ValueId,
        computation: ValueId,
        sequence: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let left = self.field(sequence, self.layout.sequence, 2, 0, body);
        let right = self.field(sequence, self.layout.sequence, 2, 1, body);
        let mut cases = Vec::new();
        for tag in 0..4 {
            let mut branch = Vec::new();
            let pending = if tag == 0 {
                right
            } else {
                let head = self.field(left, self.layout.sequence, tag, 0, &mut branch);
                let rest = self.field(left, self.layout.sequence, tag, 1, &mut branch);
                let rest = self.node(self.layout.sequence, 2, vec![rest, right], &mut branch);
                let mut fields = vec![head, rest];
                if tag == 3 {
                    fields.push(self.field(left, self.layout.sequence, tag, 2, &mut branch));
                }
                self.node(self.layout.sequence, tag, fields, &mut branch)
            };
            let result = self.recur(state, handler, computation, pending, &mut branch);
            cases.push(arm(tag as i32, branch, result));
        }
        self.switch(left, self.layout.sequence, cases, body)
    }

    pub(super) fn finish_source(
        &mut self,
        state: ValueId,
        handler: ValueId,
        value: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let context = self.context.unwrap();
        let mut branch = Vec::new();
        let result = self.pair(state, value, &mut branch);
        let root = arm(0, branch, result);
        let mut branch = Vec::new();
        let ret = self.field(context, self.layout.context, 1, 0, &mut branch);
        let target = self.apply(ret, &[value], &mut branch);
        let result = self.enter_target(state, handler, target, &mut branch);
        self.switch(
            context,
            self.layout.context,
            vec![root, arm(1, branch, result)],
            body,
        )
    }

    fn enter_target(
        &mut self,
        state: ValueId,
        handler: ValueId,
        target: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let context = self.context.unwrap();
        let saved = self.field(context, self.layout.context, 1, 2, body);
        let parent = self.field(context, self.layout.context, 1, 3, body);
        let caller = self.field(context, self.layout.context, 1, 4, body);
        let extent = self.field(context, self.layout.context, 1, 5, body);
        let finish = self.node(self.layout.sequence, 3, vec![extent, saved, parent], body);
        let old_context = self.context.replace(caller);
        let old_boundary = self.boundary.replace(extent);
        let result = self.recur(state, handler, target, finish, body);
        self.context = old_context;
        self.boundary = old_boundary;
        result
    }

    pub(super) fn requested(
        &mut self,
        state: ValueId,
        handler: ValueId,
        computation: ValueId,
        sequence: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let request = self.field(computation, self.layout.computation, 1, 0, body);
        let context = self.context.unwrap();
        let mut cases = Vec::new();
        for tag in 0..2 {
            let mut branch = Vec::new();
            let index = if tag == 0 { 0 } else { 5 };
            let extent = self.field(context, self.layout.context, tag, index, &mut branch);
            let continuation = self.closure(
                self.functions.resume,
                self.layout.continuation,
                vec![sequence, extent],
                &mut branch,
            );
            let result = if tag == 0 {
                let pair = self.apply(handler, &[state, request, continuation], &mut branch);
                let next_state = self.pair_field(pair, 0, &mut branch);
                let next = self.pair_field(pair, 1, &mut branch);
                let empty = self.node(self.layout.sequence, 0, vec![], &mut branch);
                self.recur(next_state, handler, next, empty, &mut branch)
            } else {
                let op = self.field(context, self.layout.context, 1, 1, &mut branch);
                let target = self.apply(op, &[request, continuation], &mut branch);
                self.enter_target(state, handler, target, &mut branch)
            };
            cases.push(arm(tag as i32, branch, result));
        }
        self.switch(context, self.layout.context, cases, body)
    }
}
