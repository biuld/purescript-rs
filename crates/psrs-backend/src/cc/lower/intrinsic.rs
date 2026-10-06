//! P8 closure-conversion lowering of a Core `IntrinsicCall`.
//!
//! The per-intrinsic cases live here so `lower_value_inner` has one arm for
//! every intrinsic. The array, string, and scalar helpers this dispatches to are
//! the same ones the previous per-operation expressions used.

use super::super::{Assignment, AssignmentKind, ValueId, ValueShape};
use super::FunctionLowerer;
use super::scalar::{lower_binary_op, lower_unary_op};
use crate::BackendError;
use psrs_core::Expr;
use psrs_hir::{Intrinsic, IntrinsicCategory};

impl FunctionLowerer<'_> {
    pub(super) fn lower_intrinsic(
        &mut self,
        expression: &Expr,
        intrinsic: Intrinsic,
        arguments: &[Expr],
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        match intrinsic {
            Intrinsic::NumberToString => {
                let value = self.lower_value(&arguments[0], assignments)?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::NumberToString { value },
                    span: expression.span,
                });
                Ok(destination)
            }
            Intrinsic::ArrayIndex => {
                let array = &arguments[0];
                let Some(representation) = self.array_types.get(&array.ty).copied() else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "array expression has no representation requirement",
                    )]);
                };
                let array = self.lower_value(array, assignments)?;
                let index = self.lower_value(&arguments[1], assignments)?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::ArrayGet {
                        destination,
                        representation,
                        value: array,
                        index,
                    },
                    span: expression.span,
                });
                Ok(destination)
            }
            Intrinsic::ArrayUpdate => self.lower_array_update(
                expression,
                &arguments[0],
                &arguments[1],
                &arguments[2],
                assignments,
            ),
            Intrinsic::ArrayLength => {
                let value = self.lower_value(&arguments[0], assignments)?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::ArrayLen { destination, value },
                    span: expression.span,
                });
                Ok(destination)
            }
            Intrinsic::ArrayFill => {
                self.lower_array_fill(expression, &arguments[0], &arguments[1], ty, assignments)
            }
            Intrinsic::ArrayWrite => self.lower_array_write(
                expression,
                &arguments[0],
                &arguments[1],
                &arguments[2],
                assignments,
            ),
            Intrinsic::ArrayAppend => {
                self.lower_array_append(expression, &arguments[0], &arguments[1], ty, assignments)
            }
            Intrinsic::StringToBytes => {
                self.lower_string_to_bytes(expression, &arguments[0], ty, assignments)
            }
            Intrinsic::BytesToString => {
                self.lower_bytes_to_string(expression, &arguments[0], ty, assignments)
            }
            Intrinsic::UnsafeCoerce => {
                let argument = &arguments[0];
                let source_type = argument.ty;
                let source_shape = self.value_shape(source_type, expression.span)?;
                let value = self.lower_value(argument, assignments)?;
                let conversion = self.typed_conversion(
                    source_type,
                    expression.ty,
                    source_shape,
                    ty,
                    expression.span,
                )?;
                Ok(self.emit_conversion(
                    value,
                    source_shape,
                    ty,
                    conversion,
                    expression.span,
                    assignments,
                ))
            }
            _ => match intrinsic.descriptor().category {
                IntrinsicCategory::BinaryScalar => {
                    let left = self.lower_value(&arguments[0], assignments)?;
                    let right = self.lower_value(&arguments[1], assignments)?;
                    let destination = self.fresh(ty);
                    assignments.push(Assignment {
                        destination,
                        kind: AssignmentKind::Primitive {
                            op: lower_binary_op(intrinsic),
                            left,
                            right,
                        },
                        span: expression.span,
                    });
                    Ok(destination)
                }
                IntrinsicCategory::UnaryScalar => {
                    let value = self.lower_value(&arguments[0], assignments)?;
                    let destination = self.fresh(ty);
                    assignments.push(Assignment {
                        destination,
                        kind: AssignmentKind::Unary {
                            op: lower_unary_op(intrinsic),
                            value,
                        },
                        span: expression.span,
                    });
                    Ok(destination)
                }
                _ => Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "intrinsic has no closure-conversion lowering",
                )]),
            },
        }
    }
}
