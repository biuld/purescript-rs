//! Type verification of the array expressions.
//!
//! Each rule fixes the operand and result types of one array operation. A
//! mismatch is a lowering bug rather than a source error, so every check is an
//! internal verification error.

use super::super::{array_element, compatible, error, primitive_type_id};
use super::Context;
use crate::{Expr, TypeConstructor};

impl Context<'_> {
    pub(super) fn verify_array_length(&mut self, expression: &Expr, array: &Expr) {
        self.expr(array, None);
        if array_element(array.ty, self.module).is_none() {
            self.errors.push(error(
                self.owner,
                array.span,
                "arrayLength expects an Array value",
            ));
        }
        self.shape(expression, TypeConstructor::Int);
    }

    pub(super) fn verify_array_append(&mut self, expression: &Expr, left: &Expr, right: &Expr) {
        self.expr(left, None);
        self.expr(right, None);
        if array_element(left.ty, self.module).is_none() {
            self.errors.push(error(
                self.owner,
                left.span,
                "arrayAppend expects an Array value",
            ));
            return;
        }
        if array_element(right.ty, self.module).is_none() {
            self.errors.push(error(
                self.owner,
                right.span,
                "arrayAppend expects an Array value",
            ));
            return;
        }
        compatible(
            left.ty,
            right.ty,
            self.module,
            self.owner,
            expression.span,
            self.errors,
        );
        compatible(
            left.ty,
            expression.ty,
            self.module,
            self.owner,
            expression.span,
            self.errors,
        );
    }

    pub(super) fn verify_array_fill(&mut self, expression: &Expr, length: &Expr, value: &Expr) {
        self.expr(
            length,
            Some(primitive_type_id(self.module, TypeConstructor::Int)),
        );
        let Some(element) = array_element(expression.ty, self.module) else {
            self.errors.push(error(
                self.owner,
                expression.span,
                "arrayFill must return an Array",
            ));
            return;
        };
        self.expr(value, Some(element));
    }

    pub(super) fn verify_array_index(&mut self, expression: &Expr, array: &Expr, index: &Expr) {
        let Some(element_type) = array_element(array.ty, self.module) else {
            self.errors.push(error(
                self.owner,
                array.span,
                "arrayIndex expects an Array value",
            ));
            return;
        };
        self.expr(array, None);
        self.expr(
            index,
            Some(primitive_type_id(self.module, TypeConstructor::Int)),
        );
        compatible(
            element_type,
            expression.ty,
            self.module,
            self.owner,
            expression.span,
            self.errors,
        );
    }

    pub(super) fn verify_array_update(
        &mut self,
        expression: &Expr,
        array: &Expr,
        index: &Expr,
        value: &Expr,
    ) {
        let Some(element_type) = array_element(array.ty, self.module) else {
            self.errors.push(error(
                self.owner,
                array.span,
                "arrayUpdate expects an Array value",
            ));
            return;
        };
        self.expr(array, None);
        self.expr(
            index,
            Some(primitive_type_id(self.module, TypeConstructor::Int)),
        );
        self.expr(value, Some(element_type));
        compatible(
            array.ty,
            expression.ty,
            self.module,
            self.owner,
            expression.span,
            self.errors,
        );
    }
}
