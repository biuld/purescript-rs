//! Mutable status cells are private; frame payloads and sequences stay immutable.

use super::builder::{Machine, repr};
use super::executor::arm;
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, AssignmentKind, Function, TagCase, ValueId, ValueShape};

impl Machine<'_> {
    pub(super) fn integer(&mut self, n: i32, body: &mut Vec<Assignment>) -> ValueId {
        self.emit(ValueShape::Integer, body, |_| AssignmentKind::Constant(n))
    }

    pub(super) fn new_cell(
        &mut self,
        representation: crate::cc::ReprId,
        value: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let cell = self.emit(repr(representation), body, |destination| {
            AssignmentKind::ArrayNew {
                destination,
                representation,
                elements: vec![value],
            }
        });
        self.cast(cell, erased_shape(), body)
    }

    pub(super) fn read_cell(
        &mut self,
        representation: crate::cc::ReprId,
        shape: ValueShape,
        cell: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let cell = self.cast(cell, repr(representation), body);
        let index = self.integer(0, body);
        self.emit(shape, body, |destination| AssignmentKind::ArrayGet {
            destination,
            representation,
            value: cell,
            index,
        })
    }

    pub(super) fn write_cell(
        &mut self,
        representation: crate::cc::ReprId,
        cell: ValueId,
        new_value: ValueId,
        body: &mut Vec<Assignment>,
    ) {
        let cell = self.cast(cell, repr(representation), body);
        let index = self.integer(0, body);
        body.push(Assignment {
            destination: cell,
            kind: AssignmentKind::ArraySet {
                destination: cell,
                representation,
                value: cell,
                index,
                new_value,
            },
            span: self.span,
        });
    }

    pub(super) fn new_status(&mut self, body: &mut Vec<Assignment>) -> ValueId {
        let zero = self.integer(0, body);
        self.new_cell(self.layout.status, zero, body)
    }

    pub(super) fn status(&mut self, cell: ValueId, body: &mut Vec<Assignment>) -> ValueId {
        self.read_cell(self.layout.status, ValueShape::Integer, cell, body)
    }

    pub(super) fn set_status(&mut self, cell: ValueId, n: i32, body: &mut Vec<Assignment>) {
        let new_value = self.integer(n, body);
        self.write_cell(self.layout.status, cell, new_value, body);
    }

    pub(super) fn guard_pending(&mut self, cell: ValueId, body: &mut Vec<Assignment>) {
        let status = self.status(cell, body);
        self.select(status, vec![arm(0, vec![], cell)], body);
    }

    pub(super) fn select(
        &mut self,
        value: ValueId,
        cases: Vec<TagCase>,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let trap = self.lower.fresh(erased_shape());
        let default_assignments = vec![Assignment {
            destination: trap,
            kind: AssignmentKind::Unreachable,
            span: self.span,
        }];
        self.emit(erased_shape(), body, |_| AssignmentKind::TagSwitch {
            value,
            cases,
            default_assignments,
            default_value: trap,
        })
    }

    pub(super) fn new_boundary(
        &mut self,
        parent: Option<ValueId>,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let cell = self.new_status(body);
        let (case, fields) = match parent {
            Some(parent) => (1, vec![cell, parent]),
            None => (0, vec![cell]),
        };
        self.node(self.layout.boundary, case, fields, body)
    }

    pub(super) fn check_boundary(
        &mut self,
        boundary: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let function = self.functions.check;
        self.emit(erased_shape(), body, |_| AssignmentKind::DirectCall {
            function,
            arguments: vec![boundary],
        })
    }

    pub(super) fn close_boundary(
        &mut self,
        boundary: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let function = self.functions.close;
        self.emit(erased_shape(), body, |_| AssignmentKind::DirectCall {
            function,
            arguments: vec![boundary],
        })
    }

    pub(super) fn start_boundary(mut self) -> Function {
        let mut body = Vec::new();
        let result = self.new_boundary(None, &mut body);
        let symbol = self.functions.start;
        self.finish(symbol, "control_start_boundary", vec![], body, result)
    }

    pub(super) fn boundary_check(mut self) -> Function {
        let boundary = self.lower.fresh(erased_shape());
        let mut cases = Vec::new();
        for case in 0..2 {
            let mut branch = Vec::new();
            let cell = self.field(boundary, self.layout.boundary, case, 0, &mut branch);
            self.guard_pending(cell, &mut branch);
            let result = if case == 0 {
                boundary
            } else {
                let parent = self.field(boundary, self.layout.boundary, case, 1, &mut branch);
                self.check_boundary(parent, &mut branch)
            };
            cases.push(arm(case as i32, branch, result));
        }
        let mut body = Vec::new();
        let result = self.switch(boundary, self.layout.boundary, cases, &mut body);
        let symbol = self.functions.check;
        self.finish(
            symbol,
            "control_check_boundary",
            vec![boundary],
            body,
            result,
        )
    }

    pub(super) fn boundary_close(mut self) -> Function {
        let boundary = self.lower.fresh(erased_shape());
        let mut cases = Vec::new();
        for case in 0..2 {
            let mut branch = Vec::new();
            let cell = self.field(boundary, self.layout.boundary, case, 0, &mut branch);
            self.set_status(cell, 1, &mut branch);
            cases.push(arm(case as i32, branch, boundary));
        }
        let mut body = Vec::new();
        let result = self.switch(boundary, self.layout.boundary, cases, &mut body);
        let symbol = self.functions.close;
        self.finish(
            symbol,
            "control_close_boundary",
            vec![boundary],
            body,
            result,
        )
    }
}
