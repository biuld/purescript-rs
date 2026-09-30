use super::super::super::layout::function_signature;
use super::super::super::{Assignment, AssignmentKind, ValueId};
use super::super::{FunctionLowerer, Signature, ValueShape};
use super::helpers::{
    callable_parameter_types, callable_result_type, collect_application,
    conversion_reconstructs_aggregate, function_value_types, persist_reference, restore_reference,
};
use super::partial::PartialApplication;
use super::{ApplicationLowering, CallShape};
use crate::BackendError;
use psrs_core::{Expr, ExprKind};
use psrs_span::TextRange;

impl ApplicationLowering for FunctionLowerer<'_> {
    fn lower_application(
        &mut self,
        expression: &Expr,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let (head, arguments) = collect_application(self.module, expression);
        if let ExprKind::Global(function) = head.kind {
            let signature = self.signatures.get(&function).cloned().ok_or_else(|| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    head.span,
                    "call target is not a local top-level function",
                )]
            })?;
            if arguments.len() < signature.parameters.len() {
                return self.lower_partial_global_application(
                    PartialApplication {
                        expression,
                        function,
                        source_signature: &signature,
                        arguments,
                        result_type,
                        callable_type: head.ty,
                    },
                    assignments,
                );
            }
            if arguments.len() > signature.parameters.len() {
                // A global declaration and its callable type can disagree on
                // arity when the declaration evaluates to a function value
                // rather than binding every parameter syntactically. The global
                // then behaves as a function value, so call it indirectly after
                // evaluating it, exactly like any other higher-order callee.
                return self.lower_indirect_application(
                    expression,
                    head,
                    arguments,
                    result_type,
                    assignments,
                );
            }
            self.check_call_shape(
                &signature,
                arguments.len(),
                signature.result,
                expression.span,
            )?;
            let source_parameters = callable_parameter_types(self.module, function, head.ty);
            let mut conversions = Vec::with_capacity(arguments.len());
            for (index, (argument, expected)) in
                arguments.iter().zip(&signature.parameters).enumerate()
            {
                let source_type = source_parameters.get(index).copied().ok_or_else(|| {
                    vec![BackendError::new(
                        "P8 closure conversion",
                        argument.span,
                        "call parameter has no declaration type",
                    )]
                })?;
                let source_shape = self.value_shape(argument.ty, argument.span)?;
                let conversion = self.typed_conversion(
                    argument.ty,
                    source_type,
                    source_shape,
                    *expected,
                    expression.span,
                )?;
                conversions.push((source_shape, conversion));
            }
            let mut values = Vec::with_capacity(arguments.len());
            for (index, (argument, expected)) in
                arguments.iter().zip(&signature.parameters).enumerate()
            {
                let value = self.lower_value(argument, assignments)?;
                let (source_shape, conversion) = conversions[index].clone();
                let converted = self.emit_conversion(
                    value,
                    source_shape,
                    *expected,
                    conversion,
                    expression.span,
                    assignments,
                );
                let later_reconstructs = conversions[index + 1..]
                    .iter()
                    .any(|(_, conversion)| conversion_reconstructs_aggregate(conversion));
                let (converted, restore_shape) = if later_reconstructs {
                    persist_reference(self, converted, *expected, expression.span, assignments)
                } else {
                    (converted, None)
                };
                values.push((converted, restore_shape));
            }
            let values = values
                .into_iter()
                .map(|(value, shape)| match shape {
                    Some(shape) => {
                        restore_reference(self, value, shape, expression.span, assignments)
                    }
                    None => value,
                })
                .collect();
            let call_result = self.fresh(signature.result);
            assignments.push(Assignment {
                destination: call_result,
                kind: AssignmentKind::DirectCall {
                    function,
                    arguments: values,
                },
                span: expression.span,
            });
            let source_type =
                callable_result_type(self.module, function, head.ty).ok_or_else(|| {
                    vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "call target has no declaration result type",
                    )]
                })?;
            let conversion = self.typed_conversion(
                source_type,
                expression.ty,
                signature.result,
                result_type,
                expression.span,
            )?;
            Ok(self.emit_conversion(
                call_result,
                signature.result,
                result_type,
                conversion,
                expression.span,
                assignments,
            ))
        } else {
            self.lower_indirect_application(expression, head, arguments, result_type, assignments)
        }
    }
}

impl FunctionLowerer<'_> {
    /// Evaluates a callee expression to a closure value and calls it with the
    /// supplied arguments. Used for every callee that is not a saturated
    /// top-level direct call, including over-applied globals.
    fn lower_indirect_application(
        &mut self,
        expression: &Expr,
        head: &Expr,
        arguments: Vec<&Expr>,
        result_type: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let signature = function_signature(
            self.module,
            head.ty,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )?;
        self.check_call_shape(
            &signature,
            arguments.len(),
            signature.result,
            expression.span,
        )?;
        let Some(signature_id) = self.function_types.get(&head.ty).copied() else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "higher-order call has no runtime function type",
            )]);
        };
        let (source_parameters, source_result) = function_value_types(self.module, head.ty);
        // The callee expression is lowered at its own use type, so its runtime
        // value already matches `head.ty`; no side-table adaptation is needed.
        let function = self.lower_value(head, assignments)?;
        let mut conversions = Vec::with_capacity(arguments.len());
        for (index, argument) in arguments.iter().enumerate() {
            let source_parameter = source_parameters.get(index).copied().ok_or_else(|| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    argument.span,
                    "call parameter has no source type",
                )]
            })?;
            let shape = self.value_shape(argument.ty, argument.span)?;
            let conversion = self.typed_conversion(
                argument.ty,
                source_parameter,
                shape,
                signature.parameters[index],
                expression.span,
            )?;
            conversions.push((shape, conversion));
        }
        let mut values = Vec::with_capacity(arguments.len());
        let function = if conversions
            .iter()
            .any(|(_, plan)| conversion_reconstructs_aggregate(plan))
        {
            persist_reference(
                self,
                function,
                self.value_shape(head.ty, expression.span)?,
                expression.span,
                assignments,
            )
        } else {
            (function, None)
        };
        for (index, argument) in arguments.iter().enumerate() {
            let value = self.lower_value(argument, assignments)?;
            let (shape, conversion) = conversions[index].clone();
            let expected = signature.parameters[index];
            let value = self.emit_conversion(
                value,
                shape,
                expected,
                conversion,
                expression.span,
                assignments,
            );
            let persistent = if conversions[index + 1..]
                .iter()
                .any(|(_, plan)| conversion_reconstructs_aggregate(plan))
            {
                persist_reference(self, value, expected, expression.span, assignments)
            } else {
                (value, None)
            };
            values.push(persistent);
        }
        let function = match function {
            (value, Some(shape)) => {
                restore_reference(self, value, shape, expression.span, assignments)
            }
            (value, None) => value,
        };
        let values = values
            .into_iter()
            .map(|(value, shape)| match shape {
                Some(shape) => restore_reference(self, value, shape, expression.span, assignments),
                None => value,
            })
            .collect();
        let destination = self.fresh(signature.result);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::IndirectCall {
                function,
                signature: signature_id,
                arguments: values,
            },
            span: expression.span,
        });
        if signature.result == result_type {
            return Ok(destination);
        }
        let conversion = self.typed_conversion(
            source_result,
            expression.ty,
            signature.result,
            result_type,
            expression.span,
        )?;
        let result = self.emit_conversion(
            destination,
            signature.result,
            result_type,
            conversion,
            expression.span,
            assignments,
        );
        Ok(result)
    }
}

impl CallShape for FunctionLowerer<'_> {
    fn check_call_shape(
        &self,
        signature: &Signature,
        argument_count: usize,
        result: ValueShape,
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
