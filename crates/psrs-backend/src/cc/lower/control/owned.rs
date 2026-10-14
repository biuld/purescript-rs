//! One-shot ownership is consumed at interpretation, before invoking user code.

use super::builder::{Machine, repr};
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, Function, ValueId};
use crate::control::schema::{ComputationCase, OwnerCase, computation as c, owner as o};

impl Machine<'_> {
    pub(super) fn owned(mut self) -> Function {
        let signature = self
            .lower
            .representations
            .signature(self.layout.once)
            .unwrap();
        let continuation = self.lower.fresh(signature.parameters[0]);
        let mut body = Vec::new();
        let erased = self.cast(continuation, erased_shape(), &mut body);
        let callback = self.new_cell(self.layout.callback, erased, &mut body);
        let status = self.new_status(&mut body);
        let result = self.node(
            self.layout.owner,
            OwnerCase::OneShot.tag(),
            vec![callback, status],
            &mut body,
        );
        let symbol = self.functions.once;
        self.finish(symbol, "control_once", vec![continuation], body, result)
    }

    pub(super) fn cancel(mut self) -> Function {
        let owner = self.lower.fresh(repr(self.layout.owner));
        let mut body = Vec::new();
        let status = self.field(
            owner,
            self.layout.owner,
            OwnerCase::OneShot.tag(),
            o::STATUS,
            &mut body,
        );
        self.guard_pending(status, &mut body);
        self.set_status(status, 3, &mut body);
        let callback = self.field(
            owner,
            self.layout.owner,
            OwnerCase::OneShot.tag(),
            o::CALLBACK,
            &mut body,
        );
        self.write_cell(self.layout.callback, callback, status, &mut body);
        let symbol = self.functions.cancel;
        self.finish(symbol, "control_cancel", vec![owner], body, owner)
    }

    pub(super) fn consume_owned(
        &mut self,
        state: ValueId,
        handler: ValueId,
        computation: ValueId,
        sequence: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let owner = self.field(
            computation,
            self.layout.computation,
            ComputationCase::OwnedResume.tag(),
            c::OWNED_OWNER,
            body,
        );
        let value = self.field(
            computation,
            self.layout.computation,
            ComputationCase::OwnedResume.tag(),
            c::OWNED_VALUE,
            body,
        );
        let callback = self.field(
            owner,
            self.layout.owner,
            OwnerCase::OneShot.tag(),
            o::CALLBACK,
            body,
        );
        let status = self.field(
            owner,
            self.layout.owner,
            OwnerCase::OneShot.tag(),
            o::STATUS,
            body,
        );
        self.guard_pending(status, body);
        // No guest callback runs between this check and transition. The pinned
        // guest profile is single-threaded; re-entrant callbacks see Running.
        self.set_status(status, 1, body);
        let continuation = self.read_cell(self.layout.callback, erased_shape(), callback, body);
        // The local invocation roots the callback; a retained consumed owner
        // must no longer root its captures.
        self.write_cell(self.layout.callback, callback, status, body);
        let next = self.apply(continuation, &[value], body);
        self.set_status(status, 2, body);
        self.recur(state, handler, next, sequence, body)
    }
}
