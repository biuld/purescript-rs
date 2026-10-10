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
        let ExprKind::Lambda { binder, body } = &self.lambda.kind else {
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
        let mut expressions = HashSet::new();
        let mut invocations = Vec::new();
        executed(module, body, &mut expressions, &mut invocations);
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
        let expected = super::derive(module, self.owner, self.lambda)?;
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

fn executed(
    module: &Module,
    value: &Expr,
    expressions: &mut HashSet<*const Expr>,
    invocations: &mut Vec<*const Expr>,
) -> bool {
    psrs_span::with_sufficient_stack(|| {
        expressions.insert(value as *const Expr);
        let mut visit = |value| executed(module, value, expressions, invocations);
        match &value.kind {
            ExprKind::Constructor { arguments, .. }
            | ExprKind::IntrinsicCall { arguments, .. }
            | ExprKind::Array {
                elements: arguments,
            } => arguments.iter().all(visit),
            ExprKind::Record { fields } => fields.iter().all(|(_, value)| visit(value)),
            ExprKind::RecordUpdate { record, fields } => {
                visit(record) && fields.iter().all(|(_, value)| visit(value))
            }
            ExprKind::FieldAccess { record, .. }
            | ExprKind::RepresentationCast { value: record, .. } => visit(record),
            ExprKind::Application(function, argument) => {
                if !visit(function) || !visit(argument) {
                    return false;
                }
                if crate::state::region(module, argument.ty).is_some()
                    && !super::summary::passthrough(module, function)
                {
                    invocations.push(value as *const Expr);
                }
                true
            }
            ExprKind::Let { bindings, body } => {
                bindings.iter().all(|binding| visit(&binding.value)) && visit(body)
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if !visit(condition) {
                    return false;
                }
                let left = visit(then_branch);
                let right = visit(else_branch);
                left || right
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                if !visit(scrutinee) {
                    return false;
                }
                let mut returns = false;
                for branch in branches {
                    returns |= visit(&branch.value);
                }
                returns
            }
            // Closure creation is inert, so nested execution bodies do not
            // supply transitions to this function's dependency graph.
            ExprKind::Trap => false,
            _ => true,
        }
    })
}
