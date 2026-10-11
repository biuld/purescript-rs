//! P8 closure-conversion lowering of a Core `IntrinsicCall`.
//!
//! The per-intrinsic cases live here so `lower_value_inner` has one arm for
//! every intrinsic. The array, string, and scalar helpers this dispatches to are
//! the same ones the previous per-operation expressions used.

use super::super::{Assignment, AssignmentKind, BinaryOp, ValueId, ValueShape};
use super::FunctionLowerer;
use crate::BackendError;
use crate::cc::payload::erased_shape;
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
            Implementation::Control => {
                self.lower_control(expression, intrinsic, arguments, ty, assignments)
            }
            Implementation::Elaborated | Implementation::Unsupported => {
                Err(vec![BackendError::invalid_ir(
                    "P8 closure conversion",
                    expression.span,
                    "intrinsic requires elaboration or has no runtime implementation",
                )])
            }
        }
    }

    /// Lowers a `Prim.Control` primitive to a frame in the shared control
    /// representation. `return`/`perform`/`bind` build frames; the executor
    /// (`run`/`handle`) is not lowered yet.
    pub(super) fn lower_control(
        &mut self,
        expression: &Expr,
        intrinsic: Intrinsic,
        arguments: &[Expr],
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(representation) = self.control_type else {
            return Err(vec![BackendError::invalid_ir(
                "P8 closure conversion",
                expression.span,
                "control primitive used without a frame representation",
            )]);
        };
        let (case, fields): (u32, Vec<&Expr>) = match intrinsic {
            Intrinsic::ControlReturn => (0, vec![&arguments[0]]),
            Intrinsic::ControlPerform => (1, vec![&arguments[0]]),
            Intrinsic::ControlBind => (2, vec![&arguments[0], &arguments[1]]),
            Intrinsic::ControlRun => {
                return self.lower_control_run(expression, arguments, ty, assignments);
            }
            Intrinsic::ControlHandle => {
                return Err(vec![BackendError::invalid_ir(
                    "P8 closure conversion",
                    expression.span,
                    "the open control handler is not lowered yet",
                )]);
            }
            _ => {
                return Err(vec![BackendError::invalid_ir(
                    "P8 closure conversion",
                    expression.span,
                    "not a control primitive",
                )]);
            }
        };
        let mut values = Vec::with_capacity(fields.len());
        for field in fields {
            values.push(self.lower_array_element(field, assignments)?);
        }
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::VariantNew {
                destination,
                representation,
                case,
                fields: values,
            },
            span: expression.span,
        });
        Ok(destination)
    }

    /// Lowers `ControlRun` inline: read the frame tag and, for a `return` frame,
    /// build the `(state, value)` result. `perform`/`bind` frames trap until the
    /// executor grows those cases.
    fn lower_control_run(
        &mut self,
        expression: &Expr,
        arguments: &[Expr],
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(representation) = self.control_type else {
            return Err(vec![BackendError::invalid_ir(
                "P8 closure conversion",
                expression.span,
                "control primitive used without a frame representation",
            )]);
        };
        let Some(record_representation) = self.record_types.get(&expression.ty).copied() else {
            return Err(vec![BackendError::invalid_ir(
                "P8 closure conversion",
                expression.span,
                "control run result is not a represented record",
            )]);
        };
        let value_type = self
            .module
            .record_field(expression.ty, "_2")
            .ok_or_else(|| {
                vec![BackendError::invalid_ir(
                    "P8 closure conversion",
                    expression.span,
                    "control run result record has no value field",
                )]
            })?;
        let value_shape = self.value_shape(value_type, expression.span)?;
        let state = self.lower_value(&arguments[0], assignments)?;
        let _handler = self.lower_value(&arguments[1], assignments)?;
        let computation = self.lower_value(&arguments[2], assignments)?;
        let plan = self.recover_payload(value_shape, expression.span)?;

        let zero = self.fresh(ValueShape::Integer);
        let tag = self.fresh(ValueShape::Integer);
        let condition = self.fresh(ValueShape::Boolean);
        let payload_erased = self.fresh(erased_shape());
        let mut then_assignments = vec![Assignment {
            destination: payload_erased,
            kind: AssignmentKind::VariantGet {
                destination: payload_erased,
                representation,
                case: 0,
                field: 0,
                value: computation,
            },
            span: expression.span,
        }];
        let payload = self.emit_conversion(
            payload_erased,
            erased_shape(),
            value_shape,
            plan,
            expression.span,
            &mut then_assignments,
        );
        let record = self.fresh(ty);
        then_assignments.push(Assignment {
            destination: record,
            kind: AssignmentKind::ProductNew {
                destination: record,
                representation: record_representation,
                arguments: vec![state, payload],
            },
            span: expression.span,
        });
        let trap = self.fresh(ty);
        let result = self.fresh(ty);
        assignments.push(Assignment {
            destination: zero,
            kind: AssignmentKind::Constant(0),
            span: expression.span,
        });
        assignments.push(Assignment {
            destination: tag,
            kind: AssignmentKind::VariantTag {
                destination: tag,
                representation,
                value: computation,
            },
            span: expression.span,
        });
        assignments.push(Assignment {
            destination: condition,
            kind: AssignmentKind::Primitive {
                op: BinaryOp::IntEq,
                left: tag,
                right: zero,
            },
            span: expression.span,
        });
        assignments.push(Assignment {
            destination: result,
            kind: AssignmentKind::If {
                condition,
                then_assignments,
                then_value: record,
                else_assignments: vec![Assignment {
                    destination: trap,
                    kind: AssignmentKind::Unreachable,
                    span: expression.span,
                }],
                else_value: trap,
            },
            span: expression.span,
        });
        Ok(result)
    }
}
