//! A flat driver built from a checked construction plan; only the driver
//! traverses saved continuation sequences.
//!
//! The transition program is described in [`super::plan`], checked there, and
//! expanded here. This module keeps only the executor's entry construction and
//! the `TagCase` helper shared with the other machine functions.

use super::builder::{Machine, repr};
use super::plan;
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, Function, TagCase, ValueId};

impl Machine<'_> {
    pub(super) fn executor(mut self) -> Function {
        let state = self.lower.fresh(erased_shape());
        let signature = self
            .lower
            .representations
            .signature(self.layout.executor)
            .unwrap();
        let handler = self.lower.fresh(signature.parameters[1]);
        let computation = self.lower.fresh(repr(self.layout.computation));
        let sequence = self.lower.fresh(repr(self.layout.sequence));
        let boundary = self.lower.fresh(repr(self.layout.boundary));
        let context = self.lower.fresh(repr(self.layout.context));
        let parameters = vec![state, handler, computation, sequence, boundary, context];
        let locals = [state, handler, computation, sequence, boundary, context];
        let program = plan::executor();
        plan::check(&program).expect("driver plan must be complete");
        let mut body = Vec::new();
        let result = self.expand(&program, &mut Vec::new(), &locals, &mut body);
        let representations = self.lower.representations;
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
