use super::super::record_field;
use super::super::types::array_element;
use super::super::{compatible, primitive_type_id};
use super::Context;
use crate::verify::expr::error;
use crate::{Expr, ExprKind, Type, TypeConstructor};

impl Context<'_> {
    /// A record update keeps the fields it does not mention, so every named
    /// field must be declared both on the source record and on its result.
    pub(super) fn record_update(&mut self, expression: &Expr, record: &Expr) {
        let ExprKind::RecordUpdate { fields, .. } = &expression.kind else {
            unreachable!("record update verifier received another expression")
        };
        self.expr(record, None);
        for (label, value) in fields {
            if record_field(record.ty, label, self.module).is_none() {
                self.errors.push(error(
                    self.owner,
                    value.span,
                    "record update field is not declared",
                ));
            }
            let field_type = record_field(expression.ty, label, self.module);
            self.expr(value, field_type);
            if field_type.is_none() {
                self.errors.push(error(
                    self.owner,
                    value.span,
                    "record update field is not declared",
                ));
            }
        }
    }

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

    /// Checks that an expression is `Array Int`. A polymorphic binding may
    /// still name a type variable there, so a concrete mismatch is the error.
    pub(super) fn array_of_ints(&mut self, expression: &Expr, message: &'static str) {
        let element_type = primitive_type_id(self.module, TypeConstructor::Int);
        let Some(found) = array_element(expression.ty, self.module) else {
            self.errors
                .push(error(self.owner, expression.span, message));
            return;
        };
        if !matches!(
            self.module.types.get(found.0 as usize),
            Some(Type::Variable(_))
        ) {
            compatible(
                found,
                element_type,
                self.module,
                self.owner,
                expression.span,
                self.errors,
            );
        }
    }
}
