//! Source dispatch and target lifetime are separate heap contexts.

use super::builder::Machine;
use super::executor::arm;
use crate::cc::{Assignment, ValueId};
use crate::control::schema::{ComputationCase, ContextCase, SequenceCase, computation as c};
use crate::control::schema::{context as x, sequence as q};

impl Machine<'_> {
    pub(super) fn dispatch_boundary(&mut self, body: &mut Vec<Assignment>) -> ValueId {
        let context = self.context.unwrap();
        let mut cases = Vec::new();
        for case in ContextCase::ALL {
            let mut branch = Vec::new();
            let index = match case {
                ContextCase::Root => x::ROOT_EXTENT,
                ContextCase::Scoped => x::SCOPED_EXTENT,
            };
            let extent = self.field(context, self.layout.context, case.tag(), index, &mut branch);
            cases.push(arm(case.tag(), branch, extent));
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
        let ret = self.field(
            computation,
            self.layout.computation,
            ComputationCase::Handle.tag(),
            c::HANDLE_RETURN,
            body,
        );
        let op = self.field(
            computation,
            self.layout.computation,
            ComputationCase::Handle.tag(),
            c::HANDLE_OPERATION,
            body,
        );
        let inner = self.field(
            computation,
            self.layout.computation,
            ComputationCase::Handle.tag(),
            c::HANDLE_SOURCE,
            body,
        );
        let caller = self.context.unwrap();
        let parent = self.boundary.unwrap();
        let mut cases = Vec::new();
        for case in SequenceCase::ALL {
            let mut branch = Vec::new();
            if case == SequenceCase::Append {
                let result =
                    self.normalize_handle_queue(state, handler, computation, sequence, &mut branch);
                cases.push(arm(case.tag(), branch, result));
                continue;
            }
            // A leading compiler finish marker has the same checked target
            // answer boundary as this tail-returned Handle. Share that extent
            // and remove its duplicate finish; no user continuation is skipped.
            let (saved, parent, extent) = if case == SequenceCase::Finish {
                let extent = self.field(
                    sequence,
                    self.layout.sequence,
                    case.tag(),
                    q::FINISH_EXTENT,
                    &mut branch,
                );
                let saved = self.field(
                    sequence,
                    self.layout.sequence,
                    case.tag(),
                    q::FINISH_REST,
                    &mut branch,
                );
                let parent = self.field(
                    sequence,
                    self.layout.sequence,
                    case.tag(),
                    q::FINISH_PARENT,
                    &mut branch,
                );
                (saved, parent, extent)
            } else {
                let extent = self.new_boundary(Some(parent), &mut branch);
                (sequence, parent, extent)
            };
            let context = self.node(
                self.layout.context,
                ContextCase::Scoped.tag(),
                vec![ret, op, saved, parent, caller, extent],
                &mut branch,
            );
            let empty = self.node(
                self.layout.sequence,
                SequenceCase::Empty.tag(),
                vec![],
                &mut branch,
            );
            let old_context = self.context.replace(context);
            let old_boundary = self.boundary.replace(extent);
            let result = self.recur(state, handler, inner, empty, &mut branch);
            self.context = old_context;
            self.boundary = old_boundary;
            cases.push(arm(case.tag(), branch, result));
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
        let left = self.field(
            sequence,
            self.layout.sequence,
            SequenceCase::Append.tag(),
            q::APPEND_LEFT,
            body,
        );
        let right = self.field(
            sequence,
            self.layout.sequence,
            SequenceCase::Append.tag(),
            q::APPEND_RIGHT,
            body,
        );
        let mut cases = Vec::new();
        for case in SequenceCase::ALL {
            let mut branch = Vec::new();
            let pending = if case == SequenceCase::Empty {
                right
            } else {
                let (head_field, rest_field, parent_field) = match case {
                    SequenceCase::Cons => (q::CONS_HEAD, q::CONS_TAIL, None),
                    SequenceCase::Append => (q::APPEND_LEFT, q::APPEND_RIGHT, None),
                    SequenceCase::Finish => {
                        (q::FINISH_EXTENT, q::FINISH_REST, Some(q::FINISH_PARENT))
                    }
                    SequenceCase::Empty => unreachable!("empty handle queue has no head"),
                };
                let head = self.field(
                    left,
                    self.layout.sequence,
                    case.tag(),
                    head_field,
                    &mut branch,
                );
                let rest = self.field(
                    left,
                    self.layout.sequence,
                    case.tag(),
                    rest_field,
                    &mut branch,
                );
                let rest = self.node(
                    self.layout.sequence,
                    SequenceCase::Append.tag(),
                    vec![rest, right],
                    &mut branch,
                );
                let mut fields = vec![head, rest];
                if let Some(parent_field) = parent_field {
                    fields.push(self.field(
                        left,
                        self.layout.sequence,
                        case.tag(),
                        parent_field,
                        &mut branch,
                    ));
                }
                self.node(self.layout.sequence, case.tag(), fields, &mut branch)
            };
            let result = self.recur(state, handler, computation, pending, &mut branch);
            cases.push(arm(case.tag(), branch, result));
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
        let root = arm(ContextCase::Root.tag(), branch, result);
        let mut branch = Vec::new();
        let ret = self.field(
            context,
            self.layout.context,
            ContextCase::Scoped.tag(),
            x::SCOPED_RETURN,
            &mut branch,
        );
        let target = self.apply(ret, &[value], &mut branch);
        let result = self.enter_target(state, handler, target, &mut branch);
        self.switch(
            context,
            self.layout.context,
            vec![root, arm(ContextCase::Scoped.tag(), branch, result)],
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
        let saved = self.field(
            context,
            self.layout.context,
            ContextCase::Scoped.tag(),
            x::SCOPED_SAVED,
            body,
        );
        let parent = self.field(
            context,
            self.layout.context,
            ContextCase::Scoped.tag(),
            x::SCOPED_PARENT,
            body,
        );
        let caller = self.field(
            context,
            self.layout.context,
            ContextCase::Scoped.tag(),
            x::SCOPED_CALLER,
            body,
        );
        let extent = self.field(
            context,
            self.layout.context,
            ContextCase::Scoped.tag(),
            x::SCOPED_EXTENT,
            body,
        );
        let finish = self.node(
            self.layout.sequence,
            SequenceCase::Finish.tag(),
            vec![extent, saved, parent],
            body,
        );
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
        let request = self.field(
            computation,
            self.layout.computation,
            ComputationCase::Perform.tag(),
            c::PAYLOAD,
            body,
        );
        let context = self.context.unwrap();
        let mut cases = Vec::new();
        for case in ContextCase::ALL {
            let mut branch = Vec::new();
            let index = match case {
                ContextCase::Root => x::ROOT_EXTENT,
                ContextCase::Scoped => x::SCOPED_EXTENT,
            };
            let extent = self.field(context, self.layout.context, case.tag(), index, &mut branch);
            let continuation = self.closure(
                self.functions.resume,
                self.layout.continuation,
                vec![sequence, extent],
                &mut branch,
            );
            let result = if case == ContextCase::Root {
                let pair = self.apply(handler, &[state, request, continuation], &mut branch);
                let next_state = self.pair_field(
                    pair,
                    crate::control::schema::result_pair::STATE,
                    &mut branch,
                );
                let next = self.pair_field(
                    pair,
                    crate::control::schema::result_pair::VALUE,
                    &mut branch,
                );
                let empty = self.node(
                    self.layout.sequence,
                    SequenceCase::Empty.tag(),
                    vec![],
                    &mut branch,
                );
                self.recur(next_state, handler, next, empty, &mut branch)
            } else {
                let op = self.field(
                    context,
                    self.layout.context,
                    ContextCase::Scoped.tag(),
                    x::SCOPED_OPERATION,
                    &mut branch,
                );
                let target = self.apply(op, &[request, continuation], &mut branch);
                self.enter_target(state, handler, target, &mut branch)
            };
            cases.push(arm(case.tag(), branch, result));
        }
        self.switch(context, self.layout.context, cases, body)
    }
}
