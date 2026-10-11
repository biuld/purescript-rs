//! Consume the control owner's checked contract before payload erasure.
use super::{Context, error, viewed};
use crate::Expr;

impl Context<'_> {
    pub(super) fn verify_control(
        &mut self,
        expression: &Expr,
        intrinsic: psrs_hir::Intrinsic,
        arguments: &[Expr],
    ) {
        for argument in arguments {
            self.expr(argument, None);
        }
        let argument_types = arguments
            .iter()
            .map(|argument| argument.ty)
            .collect::<Vec<_>>();
        let ids = std::iter::once(expression.ty)
            .chain(argument_types.iter().copied())
            .collect::<Vec<_>>();
        let module = viewed(self.module, self.source, &ids);
        if let Err(message) =
            crate::checked_control_contract(module, intrinsic, expression.ty, &argument_types)
        {
            self.errors
                .push(error(self.owner, expression.span, message));
        }
    }
}
