use super::super::{compatible, primitive_type_id};
use super::Context;
use crate::{Expr, TypeConstructor};

impl Context<'_> {
    pub(super) fn shape(&mut self, expression: &Expr, shape: TypeConstructor) {
        compatible(
            expression.ty,
            primitive_type_id(self.module, shape),
            self.module,
            self.owner,
            expression.span,
            self.errors,
        );
    }
}
