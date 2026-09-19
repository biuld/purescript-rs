use super::{FunctionLowerer, Signature, ValueType};
use crate::BackendError;
use psrs_core::{Expr, ExprKind};
use psrs_span::TextRange;

pub(super) trait CallShape {
    fn check_call_shape(
        &self,
        signature: &Signature,
        argument_count: usize,
        result: ValueType,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>>;
}

impl CallShape for FunctionLowerer<'_> {
    fn check_call_shape(
        &self,
        signature: &Signature,
        argument_count: usize,
        result: ValueType,
        span: TextRange,
    ) -> Result<(), Vec<BackendError>> {
        if signature.parameters.len() != argument_count {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                format!(
                    "call expects {} arguments but received {}",
                    signature.parameters.len(),
                    argument_count
                ),
            )]);
        }
        if result != signature.result {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "call result type differs from the declared function type",
            )]);
        }
        Ok(())
    }
}

pub(super) fn collect_application(expression: &Expr) -> (&Expr, Vec<&Expr>) {
    let mut arguments = Vec::new();
    let mut head = expression;
    while let ExprKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function;
    }
    arguments.reverse();
    (head, arguments)
}
