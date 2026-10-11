use super::{FunctionFlow, Operation, RegionId};
use crate::{Expr, ExprKind, Module, VerifyError};
use std::collections::HashSet;

impl FunctionFlow<'_> {
    /// Check both graph validity and its correspondence to this immutable body.
    /// A structurally valid graph alone is not a certificate for source calls.
    pub fn verify(&self, module: &Module) -> Result<(), VerifyError> {
        let fail = |span, message| VerifyError {
            module: self.owner,
            span,
            message,
        };
        if !std::ptr::eq(self.source, module) {
            return Err(fail(
                self.lambda.span,
                "state flow belongs to a different source arena",
            ));
        }
        self.graph
            .verify()
            .map_err(|error| fail(error.span, error.message))?;
        let ExprKind::Lambda { binder, .. } = &self.lambda.kind else {
            return Err(fail(self.lambda.span, "state flow has no source lambda"));
        };
        let region = crate::state::region(module, binder.ty)
            .ok_or_else(|| fail(binder.span, "state flow parameter has no checked region"))?;
        if !module.types_equivalent(region, self.region) {
            return Err(fail(binder.span, "state flow changes its source region"));
        }
        if self.graph.regions != [RegionId(0)] {
            return Err(fail(
                binder.span,
                "state graph changes its checked source region mapping",
            ));
        }
        // The source evaluator owns callback execution scopes and order.
        // Re-derive before checking a witness; a syntax walk cannot distinguish
        // closure construction from invocation through a lexical alias.
        let expected = super::derive(module, self.owner, self.lambda)?;
        let invocations = expected
            .operations
            .iter()
            .map(|operation| operation.expression as *const Expr)
            .collect::<Vec<_>>();
        let expressions = invocations.iter().copied().collect::<HashSet<_>>();
        let mut seen = HashSet::new();
        for block in &self.graph.blocks {
            for transition in &block.transitions {
                let operation = self
                    .operations
                    .get(transition.operation as usize)
                    .ok_or_else(|| {
                        fail(transition.span, "state transition has no source operation")
                    })?;
                if !seen.insert(transition.operation)
                    || operation.input != transition.input
                    || operation.output != transition.output.id
                    || operation.expression.span != transition.span
                {
                    return Err(fail(
                        transition.span,
                        "state transition disagrees with its source operation",
                    ));
                }
                self.verify_operation(module, operation, &expressions)?;
            }
        }
        if seen.len() != self.operations.len() {
            return Err(fail(
                self.lambda.span,
                "state graph omits a checked source operation",
            ));
        }
        let actual = self
            .operations
            .iter()
            .map(|operation| operation.expression as *const Expr)
            .collect::<Vec<_>>();
        if actual != invocations {
            return Err(fail(
                self.lambda.span,
                "state graph changes executing source invocation order or coverage",
            ));
        }
        // Re-derive the source relation so matching call identities alone
        // cannot authorize different argument provenance or branch placement.
        // A transformed IR needs its own correspondence verifier; it cannot
        // mutate this source certificate and retain a checked claim.
        if self.graph != expected.graph {
            return Err(fail(
                self.lambda.span,
                "state graph changes source dependency provenance or control flow",
            ));
        }
        Ok(())
    }

    fn verify_operation(
        &self,
        module: &Module,
        operation: &Operation<'_>,
        expressions: &HashSet<*const Expr>,
    ) -> Result<(), VerifyError> {
        let expression = operation.expression;
        let fail = |message| VerifyError {
            module: self.owner,
            span: expression.span,
            message,
        };
        if !expressions.contains(&(expression as *const Expr)) {
            return Err(fail(
                "state operation does not belong to its executing source body",
            ));
        }
        let ExprKind::Application(function, argument) = &expression.kind else {
            return Err(fail(
                "state transition does not address an actual invocation",
            ));
        };
        let signature = crate::state::signature(module, function.ty).map_err(fail)?;
        let (state, payload) = crate::state::step(module, expression.ty)
            .ok_or_else(|| fail("state operation result has no checked Step contract"))?;
        if !signature.parameters.is_empty()
            || !module.types_equivalent(signature.region, self.region)
            || !module.types_equivalent(signature.state, argument.ty)
            || !module.types_equivalent(signature.state, state)
            || !module.types_equivalent(signature.payload, payload)
        {
            return Err(fail(
                "state operation disagrees with its checked invocation contract",
            ));
        }
        Ok(())
    }
}
