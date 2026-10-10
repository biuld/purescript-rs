//! P8 closure-conversion lowering of a Core `IntrinsicCall`.
//!
//! The per-intrinsic cases live here so `lower_value_inner` has one arm for
//! every intrinsic. The array, string, and scalar helpers this dispatches to are
//! the same ones the previous per-operation expressions used.

use super::super::{Assignment, AssignmentKind, ValueId, ValueShape};
use super::FunctionLowerer;
use crate::BackendError;
use crate::target_intrinsics::{
    GeneratedOperation as G, Implementation, ScalarOperation, implementation,
};
use psrs_core::Expr;
use psrs_hir::Intrinsic;

impl FunctionLowerer<'_> {
    pub(super) fn lower_intrinsic(
        &mut self,
        expression: &Expr,
        intrinsic: Intrinsic,
        arguments: &[Expr],
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        match implementation(intrinsic) {
            Implementation::Artifact(_) => {
                let values = arguments
                    .iter()
                    .map(|argument| self.lower_value(argument, assignments))
                    .collect::<Result<Vec<_>, _>>()?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::RuntimeCall {
                        intrinsic,
                        arguments: values,
                    },
                    span: expression.span,
                });
                Ok(destination)
            }
            Implementation::Generated(G::ArrayIndex) => {
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
                let stored_shape = crate::cc::payload::erased_shape();
                let destination = self.fresh(stored_shape);
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
                let plan = self.recover_payload(ty, expression.span)?;
                Ok(self.emit_conversion(
                    destination,
                    stored_shape,
                    ty,
                    plan,
                    expression.span,
                    assignments,
                ))
            }
            Implementation::Generated(G::ArrayUpdate) => self.lower_array_update(
                expression,
                &arguments[0],
                &arguments[1],
                &arguments[2],
                assignments,
            ),
            Implementation::Generated(G::ArrayLength) => {
                let value = self.lower_value(&arguments[0], assignments)?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::ArrayLen { destination, value },
                    span: expression.span,
                });
                Ok(destination)
            }
            Implementation::Generated(G::ArrayFill) => {
                self.lower_array_fill(expression, &arguments[0], &arguments[1], ty, assignments)
            }
            Implementation::Generated(G::ArrayWrite) => self.lower_array_write(
                expression,
                &arguments[0],
                &arguments[1],
                &arguments[2],
                assignments,
            ),
            Implementation::Generated(G::ArrayAppend) => {
                self.lower_array_append(expression, &arguments[0], &arguments[1], ty, assignments)
            }
            Implementation::Generated(G::StringToBytes) => {
                self.lower_string_to_bytes(expression, &arguments[0], ty, assignments)
            }
            Implementation::Generated(G::BytesToString) => {
                self.lower_bytes_to_string(expression, &arguments[0], ty, assignments)
            }
            Implementation::Generated(G::UnsafeCoerce) => {
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
            Implementation::Direct(operation) => {
                let destination = self.fresh(ty);
                let kind = match operation {
                    ScalarOperation::Unary(op) => AssignmentKind::Unary {
                        op,
                        value: self.lower_value(&arguments[0], assignments)?,
                    },
                    ScalarOperation::Binary(op) => AssignmentKind::Primitive {
                        op,
                        left: self.lower_value(&arguments[0], assignments)?,
                        right: self.lower_value(&arguments[1], assignments)?,
                    },
                };
                assignments.push(Assignment {
                    destination,
                    kind,
                    span: expression.span,
                });
                Ok(destination)
            }
            Implementation::Elaborated | Implementation::Unsupported => {
                Err(vec![BackendError::invalid_ir(
                    "P8 closure conversion",
                    expression.span,
                    "intrinsic requires elaboration or has no runtime implementation",
                )])
            }
            Implementation::StateExecutionBoundary => {
                psrs_core::state::primitive::verify(
                    self.module,
                    intrinsic,
                    &arguments.iter().map(|value| value.ty).collect::<Vec<_>>(),
                    expression.ty,
                )
                .map_err(|message| {
                    vec![BackendError::invalid_ir(
                        "P8 closure conversion",
                        expression.span,
                        message,
                    )]
                })?;
                let function = self.lower_value(&arguments[0], assignments)?;
                let signature = self
                    .function_types
                    .get(&arguments[0].ty)
                    .copied()
                    .ok_or_else(|| {
                        vec![BackendError::invalid_ir(
                            "P8 closure conversion",
                            expression.span,
                            "state execution has no checked callable signature",
                        )]
                    })?;
                let destination = self.fresh(ty);
                assignments.push(Assignment {
                    destination,
                    kind: AssignmentKind::StateExecution {
                        operation: intrinsic.state_operation().unwrap(),
                        function,
                        signature,
                    },
                    span: expression.span,
                });
                Ok(destination)
            }
        }
    }
}
