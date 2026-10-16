//! A flat driver built from a checked construction plan; only the driver
//! traverses saved continuation sequences.
//!
//! The transition program is described in [`super::plan`], checked there, and
//! expanded here. This module keeps only the executor's entry construction and
//! the `TagCase` helper shared with the other machine functions.

use super::builder::{Machine, repr};
use super::fields::ControlEmitter;
use super::plan;
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, Function, TagCase, ValueId};

impl Machine<'_> {
    pub(super) fn executor(mut self) -> Function {
        let state = self.fresh_value(erased_shape());
        let signature = self
            .representations
            .signature(self.layout.executor)
            .unwrap();
        let handler = self.fresh_value(signature.parameters[1]);
        let computation = self.fresh_value(repr(self.layout.computation));
        let sequence = self.fresh_value(repr(self.layout.sequence));
        let boundary = self.fresh_value(repr(self.layout.boundary));
        let context = self.fresh_value(repr(self.layout.context));
        let parameters = vec![state, handler, computation, sequence, boundary, context];
        let locals = [state, handler, computation, sequence, boundary, context];
        let program = plan::build(plan::Program::Executor);
        let mut body = Vec::new();
        let result = self.expand(&program, &locals, &mut body);
        let representations = self.representations;
        let symbol = self.functions.executor;
        let function = self.finish(symbol, "control_executor", parameters, body, result);
        plan::check_boundary(&function, representations, &program)
            .expect("executor boundary check must pass");
        function
    }
}

pub(super) fn arm(tag: u32, assignments: Vec<Assignment>, value: ValueId) -> TagCase {
    TagCase {
        tag: tag as i32,
        assignments,
        value,
    }
}
