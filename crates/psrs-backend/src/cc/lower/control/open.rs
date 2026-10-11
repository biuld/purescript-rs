//! Open interpretation stops at residual requests and saves its own delimiter.

use super::builder::Machine;
use super::executor::arm;
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, Function, ValueId};
use psrs_hir::SymbolId;

impl Machine<'_> {
    pub(super) fn requested(
        &mut self,
        state: ValueId,
        handler: ValueId,
        computation: ValueId,
        sequence: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let request = self.field(computation, self.layout.computation, 1, 0, body);
        let Some((project, inject)) = self.open else {
            return self.handled(state, handler, request, sequence, body);
        };
        let left = self.closure(
            self.functions.project_left,
            self.layout.continuation,
            vec![],
            body,
        );
        let right = self.closure(
            self.functions.project_right,
            self.layout.continuation,
            vec![],
            body,
        );
        let projected = self.apply(project, &[request, left, right], body);
        let mut branch = Vec::new();
        let request = self.field(projected, self.layout.projected, 0, 0, &mut branch);
        let result = self.handled(state, handler, request, sequence, &mut branch);
        let handled = arm(0, branch, result);

        let mut branch = Vec::new();
        let request = self.field(projected, self.layout.projected, 1, 0, &mut branch);
        let request = self.node(self.layout.computation, 1, vec![request], &mut branch);
        let continuation = self.closure(
            self.functions.forward,
            self.layout.continuation,
            vec![state, handler, project, inject, sequence],
            &mut branch,
        );
        let result = self.node(
            self.layout.computation,
            2,
            vec![request, continuation],
            &mut branch,
        );
        let forwarded = arm(1, branch, result);
        self.switch(
            projected,
            self.layout.projected,
            vec![handled, forwarded],
            body,
        )
    }

    fn handled(
        &mut self,
        state: ValueId,
        handler: ValueId,
        request: ValueId,
        sequence: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let continuation = self.closure(
            self.functions.resume,
            self.layout.continuation,
            vec![sequence],
            body,
        );
        let pair = self.apply(handler, &[state, request, continuation], body);
        let next_state = self.pair_field(pair, 0, body);
        let next = self.pair_field(pair, 1, body);
        // Abandoned work is reachable only through the clause's resumption.
        let empty = self.node(self.layout.sequence, 0, vec![], body);
        self.recur(next_state, handler, next, empty, body)
    }

    pub(super) fn forward(mut self) -> Function {
        let receiver = self.lower.fresh(super::super::lambda::closure_value_type());
        let value = self.lower.fresh(erased_shape());
        let mut body = Vec::new();
        let mut fields = (0..4)
            .map(|index| self.capture(receiver, index, &mut body))
            .collect::<Vec<_>>();
        let saved = self.capture(receiver, 4, &mut body);
        let resumed = self.node(self.layout.computation, 3, vec![value, saved], &mut body);
        fields.push(resumed);
        let result = self.node(self.layout.computation, 4, fields, &mut body);
        let symbol = self.functions.forward;
        self.finish(
            symbol,
            "control_forward",
            vec![receiver, value],
            body,
            result,
        )
    }

    pub(super) fn selector(mut self, case: u32, symbol: SymbolId) -> Function {
        let receiver = self.lower.fresh(super::super::lambda::closure_value_type());
        let value = self.lower.fresh(erased_shape());
        let mut body = Vec::new();
        let result = self.node(self.layout.projected, case, vec![value], &mut body);
        self.finish(
            symbol,
            "control_project",
            vec![receiver, value],
            body,
            result,
        )
    }
}
