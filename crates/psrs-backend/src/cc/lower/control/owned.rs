//! One-shot ownership is consumed at interpretation, before invoking user code.
//!
//! The factory and cancellation bodies are built from the checked plans in
//! [`super::plan`].

use super::builder::{Machine, repr};
use crate::cc::Function;

impl Machine<'_> {
    pub(super) fn owned(mut self) -> Function {
        let signature = self
            .lower
            .representations
            .signature(self.layout.once)
            .unwrap();
        let continuation = self.lower.fresh(signature.parameters[0]);
        let params = [continuation];
        let program = super::plan::once();
        super::plan::verify_plan(&program).expect("once plan must be well-formed");
        let mut body = Vec::new();
        let result = self.expand(&program, &mut Vec::new(), &params, &mut body);
        let representations = self.lower.representations;
        let symbol = self.functions.once;
        let function = self.finish(symbol, "control_once", vec![continuation], body, result);
        super::plan::check_boundary(&function, representations, &program)
            .expect("once boundary check must pass");
        function
    }

    pub(super) fn cancel(mut self) -> Function {
        let owner = self.lower.fresh(repr(self.layout.owner));
        let params = [owner];
        let program = super::plan::cancel();
        super::plan::verify_plan(&program).expect("cancel plan must be well-formed");
        let mut body = Vec::new();
        let result = self.expand(&program, &mut Vec::new(), &params, &mut body);
        let representations = self.lower.representations;
        let symbol = self.functions.cancel;
        let function = self.finish(symbol, "control_cancel", vec![owner], body, result);
        super::plan::check_boundary(&function, representations, &program)
            .expect("cancel boundary check must pass");
        function
    }
}
