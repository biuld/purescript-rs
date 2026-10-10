//! Type verification of an intrinsic call.
//!
//! An operation's rule is fixed by its descriptor: the scalar forms reuse the
//! primitive type tables, and the array, string, and byte forms reuse the same
//! rules as the expressions they replaced. A mismatch is a lowering bug rather
//! than a source error, so every check is an internal verification error.

use super::super::types::{primitive_type_id, primitive_types, unary_primitive_types};
use super::super::{compatible, error};
use super::Context;
use crate::{Expr, TypeConstructor};
use psrs_hir::{Intrinsic, IntrinsicCategory};

impl Context<'_> {
    pub(super) fn verify_intrinsic(
        &mut self,
        expression: &Expr,
        intrinsic: Intrinsic,
        arguments: &[Expr],
    ) {
        if intrinsic.state_operation().is_some() {
            for argument in arguments {
                self.expr(argument, None);
            }
            if let Err(message) = crate::state::primitive::verify(
                self.module,
                intrinsic,
                &arguments
                    .iter()
                    .map(|argument| argument.ty)
                    .collect::<Vec<_>>(),
                expression.ty,
            ) {
                self.errors
                    .push(error(self.owner, expression.span, message));
            }
            return;
        }
        match intrinsic {
            Intrinsic::ArrayLength => self.verify_array_length(expression, &arguments[0]),
            Intrinsic::ArrayIndex => {
                self.verify_array_index(expression, &arguments[0], &arguments[1])
            }
            Intrinsic::ArrayFill => {
                self.verify_array_fill(expression, &arguments[0], &arguments[1])
            }
            Intrinsic::ArrayWrite | Intrinsic::ArrayUpdate => {
                self.verify_array_update(expression, &arguments[0], &arguments[1], &arguments[2])
            }
            Intrinsic::ArrayAppend => {
                self.verify_array_append(expression, &arguments[0], &arguments[1])
            }
            Intrinsic::StringToBytes => self.verify_string_to_bytes(expression, &arguments[0]),
            Intrinsic::BytesToString => self.verify_bytes_to_string(expression, &arguments[0]),
            Intrinsic::NumberNaN | Intrinsic::NumberInfinity => {
                if !arguments.is_empty() {
                    self.errors.push(error(
                        self.owner,
                        expression.span,
                        "a numeric constant has no arguments",
                    ));
                }
                let result = primitive_type_id(self.module, TypeConstructor::Number);
                compatible(
                    result,
                    expression.ty,
                    self.module,
                    self.owner,
                    expression.span,
                    self.errors,
                );
            }
            _ => match intrinsic.descriptor().category {
                IntrinsicCategory::BinaryScalar => {
                    let (operand, result) = primitive_types(intrinsic, self.module);
                    self.expr(&arguments[0], Some(operand));
                    self.expr(&arguments[1], Some(operand));
                    compatible(
                        result,
                        expression.ty,
                        self.module,
                        self.owner,
                        expression.span,
                        self.errors,
                    );
                }
                IntrinsicCategory::Unary => {
                    let (operand, result) = unary_primitive_types(intrinsic, self.module);
                    self.expr(&arguments[0], Some(operand));
                    compatible(
                        result,
                        expression.ty,
                        self.module,
                        self.owner,
                        expression.span,
                        self.errors,
                    );
                }
                _ => {
                    for argument in arguments {
                        self.expr(argument, None);
                    }
                }
            },
        }
    }
}
