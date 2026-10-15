//! Mutable status cells are private; frame payloads and sequences stay immutable.

use super::builder::{Machine, repr};
use super::executor::arm;
use super::fields::ControlEmitter;
use crate::cc::{Assignment, AssignmentKind, Function, TagCase, ValueId, ValueShape};
use crate::control::schema::ExtentCase;

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
        self.emit(repr(representation), body, |destination| {
            AssignmentKind::ArrayNew {
                destination,
                representation,
                elements: vec![value],
            }
        })
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
        let Some(crate::cc::Representation::Array { element }) =
            self.representations.representation(representation)
        else {
            unreachable!("machine cell is an array");
        };
        let new_value = self
            .control_values(vec![new_value], vec![*element], self.span, body)
            .expect("machine write must preserve its cell representation")[0];
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
        let shape = self.value_shape(cases.first().expect("machine select has cases").value);
        let trap = self.fresh_value(shape);
        let default_assignments = vec![Assignment {
            destination: trap,
            kind: AssignmentKind::Unreachable,
            span: self.span,
        }];
        self.emit(shape, body, |_| AssignmentKind::TagSwitch {
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
            Some(parent) => (ExtentCase::Scoped.tag(), vec![cell, parent]),
            None => (ExtentCase::Root.tag(), vec![cell]),
        };
        self.node(self.layout.boundary, case, fields, body)
    }

    pub(super) fn check_boundary(
        &mut self,
        boundary: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let function = self.functions.check;
        self.call(function, vec![boundary], body)
    }

    pub(super) fn close_boundary(
        &mut self,
        boundary: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let function = self.functions.close;
        self.call(function, vec![boundary], body)
    }

    pub(super) fn start_boundary(mut self) -> Function {
        let program = super::plan::build(super::plan::Program::Start);
        let mut body = Vec::new();
        let result = self.expand(&program, &[], &mut body);
        let representations = self.representations;
        let symbol = self.functions.start;
        let function = self.finish(symbol, "control_start_boundary", vec![], body, result);
        super::plan::check_boundary(&function, representations, &program)
            .expect("start_boundary boundary check must pass");
        function
    }

    pub(super) fn boundary_check(mut self) -> Function {
        let boundary = self.fresh_value(repr(self.layout.boundary));
        let params = [boundary];
        let program = super::plan::build(super::plan::Program::Check);
        let mut body = Vec::new();
        let result = self.expand(&program, &params, &mut body);
        let representations = self.representations;
        let symbol = self.functions.check;
        let function = self.finish(
            symbol,
            "control_check_boundary",
            vec![boundary],
            body,
            result,
        );
        super::plan::check_boundary(&function, representations, &program)
            .expect("boundary_check boundary check must pass");
        function
    }

    pub(super) fn boundary_close(mut self) -> Function {
        let boundary = self.fresh_value(repr(self.layout.boundary));
        let params = [boundary];
        let program = super::plan::build(super::plan::Program::Close);
        let mut body = Vec::new();
        let result = self.expand(&program, &params, &mut body);
        let representations = self.representations;
        let symbol = self.functions.close;
        let function = self.finish(
            symbol,
            "control_close_boundary",
            vec![boundary],
            body,
            result,
        );
        super::plan::check_boundary(&function, representations, &program)
            .expect("boundary_close boundary check must pass");
        function
    }
}
