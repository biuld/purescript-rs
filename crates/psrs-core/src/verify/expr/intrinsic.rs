//! Type verification of an intrinsic call.
//!
//! An operation's rule is fixed by its descriptor: the scalar forms reuse the
//! primitive type tables, and the array, string, and byte forms reuse the same
//! rules as the expressions they replaced. A mismatch is a lowering bug rather
//! than a source error, so every check is an internal verification error.

use super::super::types::{primitive_types, unary_primitive_types};
use super::{Context, compatible};
use crate::Expr;
use psrs_hir::{Intrinsic, IntrinsicCategory};

impl Context<'_> {
    pub(super) fn verify_intrinsic(
        &mut self,
        expression: &Expr,
        intrinsic: Intrinsic,
        arguments: &[Expr],
    ) {
        match intrinsic {
            Intrinsic::ArrayLength => self.verify_array_length(expression, &arguments[0]),
            Intrinsic::ArrayIndex => {
                self.verify_array_index(expression, &arguments[0], &arguments[1])
            }
            Intrinsic::ArrayUpdate => {
                self.verify_array_update(expression, &arguments[0], &arguments[1], &arguments[2])
            }
            Intrinsic::ArrayAppend => {
                self.verify_array_append(expression, &arguments[0], &arguments[1])
            }
            Intrinsic::StringToBytes => self.verify_string_to_bytes(expression, &arguments[0]),
            Intrinsic::BytesToString => self.verify_bytes_to_string(expression, &arguments[0]),
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
                IntrinsicCategory::UnaryScalar => {
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
