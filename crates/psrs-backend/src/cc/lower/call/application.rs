use super::super::super::layout::{depends_on_type_variable, function_signature};
use super::super::super::{Assignment, AssignmentKind, RefShape, Reference, ValueId};
use super::super::{FunctionLowerer, Signature, ValueShape};
use super::helpers::{
    callable_parameter_types, callable_result_type, collect_application,
    conversion_reconstructs_aggregate, is_erased_value_type, is_function_type,
    is_generic_function_type, persist_reference, restore_reference,
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
        let (head, arguments) = collect_application(expression);
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
            self.check_call_shape(
                &signature,
                arguments.len(),
                signature.result,
                expression.span,
            )?;
            let source_parameters = callable_parameter_types(self.module, function, head.ty);
            let returned_erased_function_type = if is_erased_value_type(signature.result)
                && is_function_type(self.module, expression.ty)
            {
                source_parameters
                    .iter()
                    .zip(arguments.iter())
                    .find(|(source_type, argument)| {
                        is_generic_function_type(self.module, **source_type)
                            && is_function_type(self.module, argument.ty)
                    })
                    .map(|(_, argument)| argument.ty)
                    .or_else(|| {
                        depends_on_type_variable(self.module, expression.ty)
                            .then_some(expression.ty)
                    })
            } else {
                None
            };
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
                if is_generic_function_type(self.module, source_type)
                    && is_function_type(self.module, argument.ty)
                {
                    conversions.push(None);
                } else {
                    let source_shape = self.value_shape(argument.ty, argument.span)?;
                    let conversion = self.typed_conversion(
                        argument.ty,
                        source_type,
                        source_shape,
                        *expected,
                        expression.span,
                    )?;
                    conversions.push(Some((source_shape, conversion)));
                }
            }
            let mut values = Vec::with_capacity(arguments.len());
            for (index, (argument, expected)) in
                arguments.iter().zip(&signature.parameters).enumerate()
            {
                let value = self.lower_value(argument, assignments)?;
                let Some(source_type) = source_parameters.get(index).copied() else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        argument.span,
                        "call parameter has no declaration type",
                    )]);
                };
                let converted = if is_generic_function_type(self.module, source_type)
                    && is_function_type(self.module, argument.ty)
                {
                    self.adapt_erased_function_value(
                        value,
                        argument.ty,
                        source_type,
                        expression.span,
                        assignments,
                    )?
                } else {
                    let (source_shape, conversion) = conversions[index]
                        .clone()
                        .expect("non-function argument has a conversion plan");
                    self.emit_conversion(
                        value,
                        source_shape,
                        *expected,
                        conversion,
                        expression.span,
                        assignments,
                    )
                };
                let later_reconstructs = conversions[index + 1..]
                    .iter()
                    .flatten()
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
            if is_erased_value_type(signature.result) && result_type != signature.result {
                let result = self.unbox_erased_value(
                    call_result,
                    result_type,
                    expression.span,
                    assignments,
                )?;
                if let Some(function_type) = returned_erased_function_type.or_else(|| {
                    is_function_type(self.module, expression.ty).then_some(expression.ty)
                }) {
                    self.erased_function_types.insert(result, function_type);
                }
                Ok(result)
            } else {
                let source_type =
                    callable_result_type(self.module, function, head.ty).ok_or_else(|| {
                        vec![BackendError::new(
                            "P8 closure conversion",
                            expression.span,
                            "call target has no declaration result type",
                        )]
                    })?;
                let source_shape = signature.result;
                let conversion = self.typed_conversion(
                    source_type,
                    expression.ty,
                    source_shape,
                    result_type,
                    expression.span,
                )?;
                let result = self.emit_conversion(
                    call_result,
                    source_shape,
                    result_type,
                    conversion,
                    expression.span,
                    assignments,
                );
                if let Some(function_type) = returned_erased_function_type.or_else(|| {
                    is_function_type(self.module, expression.ty).then_some(expression.ty)
                }) {
                    self.erased_function_types.insert(result, function_type);
                }
                Ok(result)
            }
        } else {
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
            self.check_call_shape(&signature, arguments.len(), result_type, expression.span)?;
            let Some(signature_id) = self.function_types.get(&head.ty).copied() else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "higher-order call has no runtime function type",
                )]);
            };
            let function = self.lower_value(head, assignments)?;
            let function = if let Some(source_type) =
                self.erased_function_types.get(&function).copied()
                && source_type != head.ty
            {
                self.adapt_erased_function_value(
                    function,
                    source_type,
                    head.ty,
                    expression.span,
                    assignments,
                )?
            } else {
                function
            };
            let function = if is_generic_function_type(self.module, head.ty) {
                let cast = self.fresh(ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Closure(signature_id),
                }));
                assignments.push(Assignment {
                    destination: cast,
                    kind: AssignmentKind::RepresentationCast {
                        destination: cast,
                        value: function,
                        reference: Reference {
                            nullable: false,
                            heap: RefShape::Closure(signature_id),
                        },
                    },
                    span: expression.span,
                });
                cast
            } else {
                function
            };
            let values = arguments
                .into_iter()
                .map(|argument| self.lower_value(argument, assignments))
                .collect::<Result<Vec<_>, _>>()?;
            let destination = self.fresh(result_type);
            assignments.push(Assignment {
                destination,
                kind: AssignmentKind::IndirectCall {
                    function,
                    signature: signature_id,
                    arguments: values,
                },
                span: expression.span,
            });
            Ok(destination)
        }
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
