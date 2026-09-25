use super::super::super::{
    Assignment, AssignmentKind, Function, RefShape, Reference, ValueConversion, ValueId,
};
use super::super::lambda::LambdaLowering;
use super::super::{FunctionLowerer, Signature, ValueShape};
use super::helpers::{
    callable_parameter_types, callable_result_type, closure_value_type, closure_value_type_for,
    conversion_reconstructs_aggregate, function_result_type, is_function_type,
    is_generic_function_type, persist_reference, restore_reference,
};
use crate::BackendError;
use psrs_core::Expr;
use psrs_core::TypeId;
use psrs_hir::SymbolId;

pub(super) struct PartialApplication<'a> {
    pub(super) expression: &'a Expr,
    pub(super) function: SymbolId,
    pub(super) source_signature: &'a Signature,
    pub(super) arguments: Vec<&'a Expr>,
    pub(super) result_type: ValueShape,
    pub(super) callable_type: TypeId,
}

impl FunctionLowerer<'_> {
    pub(super) fn lower_partial_global_application(
        &mut self,
        application: PartialApplication<'_>,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let PartialApplication {
            expression,
            function,
            source_signature,
            arguments,
            result_type,
            callable_type,
        } = application;
        let Some(&target_signature_id) = self.function_types.get(&expression.ty) else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "partial application has no runtime function type",
            )]);
        };
        let Some(target_signature) = self.representations.signature(target_signature_id).cloned()
        else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "partial application has no target call signature",
            )]);
        };

        let declared_parameters = callable_parameter_types(self.module, function, callable_type);
        if declared_parameters.len() != source_signature.parameters.len() {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "partial application has an incomplete declaration signature",
            )]);
        }
        let capture_conversions = arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                let source_type = declared_parameters[index];
                let expected = source_signature.parameters[index];
                if is_generic_function_type(self.module, source_type)
                    && is_function_type(self.module, argument.ty)
                {
                    Ok(None)
                } else {
                    let source_shape = self.value_shape(argument.ty, argument.span)?;
                    let conversion = self.typed_conversion(
                        argument.ty,
                        source_type,
                        source_shape,
                        expected,
                        expression.span,
                    )?;
                    Ok(Some((source_shape, conversion)))
                }
            })
            .collect::<Result<Vec<_>, Vec<BackendError>>>()?;
        let mut captured = Vec::with_capacity(arguments.len());
        for (index, argument) in arguments.iter().enumerate() {
            let value = self.lower_value(argument, assignments)?;
            let source_type = declared_parameters[index];
            let expected = source_signature.parameters[index];
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
                let (source_shape, conversion) = capture_conversions[index]
                    .clone()
                    .expect("non-function capture has a conversion plan");
                self.emit_conversion(
                    value,
                    source_shape,
                    expected,
                    conversion,
                    expression.span,
                    assignments,
                )
            };
            let later_reconstructs = capture_conversions[index + 1..]
                .iter()
                .flatten()
                .any(|(_, conversion)| conversion_reconstructs_aggregate(conversion));
            let (converted, restore_shape) = if later_reconstructs {
                persist_reference(self, converted, expected, expression.span, assignments)
            } else {
                (converted, None)
            };
            captured.push((converted, restore_shape));
        }
        let captured = captured
            .into_iter()
            .map(|(value, shape)| match shape {
                Some(shape) => restore_reference(self, value, shape, expression.span, assignments),
                None => value,
            })
            .collect::<Vec<_>>();

        let mut nested = self.child_lowerer();
        let closure_parameter = nested.fresh(closure_value_type());
        let mut parameters = vec![closure_parameter];
        let mut nested_assignments = Vec::with_capacity(captured.len() + 1);
        let mut call_arguments = Vec::with_capacity(source_signature.parameters.len());
        let mut remaining_parameters = Vec::with_capacity(target_signature.parameters.len());
        for expected in target_signature.parameters {
            let parameter = nested.fresh(expected);
            parameters.push(parameter);
            remaining_parameters.push(parameter);
        }
        for (index, value) in captured.iter().enumerate() {
            let Some(capture_type) = self
                .values
                .iter()
                .find(|declaration| declaration.id == *value)
                .map(|declaration| declaration.ty)
            else {
                return Err(vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "partial application capture has no runtime type",
                )]);
            };
            let destination = nested.fresh(capture_type);
            nested_assignments.push(Assignment {
                destination,
                kind: AssignmentKind::ClosureGetCapture {
                    closure: closure_parameter,
                    index: index as u32,
                },
                span: expression.span,
            });
            call_arguments.push(destination);
        }
        call_arguments.extend(remaining_parameters);
        let result = nested.fresh(source_signature.result);
        nested_assignments.push(Assignment {
            destination: result,
            kind: AssignmentKind::DirectCall {
                function,
                arguments: call_arguments,
            },
            span: expression.span,
        });
        // The callee's declaration result is the un-instantiated shape, but the
        // closure signature is the partially applied expression's result. A
        // polymorphic declaration such as `pure :: a -> Effect a` returns an
        // erased value while the expression's result is concrete, so convert
        // before the generated function returns.
        let result_conversion = if source_signature.result == target_signature.result {
            ValueConversion::Identity
        } else {
            let source_result_type = callable_result_type(self.module, function, callable_type)
                .ok_or_else(|| {
                    vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "partial application callee has no declaration result type",
                    )]
                })?;
            let destination_result_type = function_result_type(self.module, expression.ty);
            self.typed_conversion(
                source_result_type,
                destination_result_type,
                source_signature.result,
                target_signature.result,
                expression.span,
            )?
        };
        let converted_result = nested.emit_conversion(
            result,
            source_signature.result,
            target_signature.result,
            result_conversion,
            expression.span,
            &mut nested_assignments,
        );

        // Allocate from the shared, collision-free symbol space. Deriving the
        // symbol from the span and the linked module id collided across linked
        // source modules whose partial applications share a source offset.
        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let generated = Function {
            symbol,
            name: format!("partial_{}", expression.span.start),
            parameters,
            values: nested.values,
            assignments: nested_assignments,
            result: converted_result,
            result_type: target_signature.result,
            span: expression.span,
        };
        crate::cc::verify::verify_function(&generated, self.signatures, self.representations)?;
        self.generated.extend(nested.generated);
        self.generated.push(generated);

        let closure = self.fresh(closure_value_type_for(target_signature_id));
        assignments.push(Assignment {
            destination: closure,
            kind: AssignmentKind::FunctionRef {
                function: symbol,
                signature: target_signature_id,
                captures: captured,
            },
            span: expression.span,
        });
        if result_type
            == ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            })
        {
            let erased = self.fresh(result_type);
            assignments.push(Assignment {
                destination: erased,
                kind: AssignmentKind::RepresentationCast {
                    destination: erased,
                    value: closure,
                    reference: Reference {
                        nullable: false,
                        heap: RefShape::Erased,
                    },
                },
                span: expression.span,
            });
            self.erased_function_types.insert(erased, expression.ty);
            Ok(erased)
        } else if result_type == closure_value_type_for(target_signature_id) {
            Ok(closure)
        } else {
            Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "partial application result has the wrong runtime type",
            )])
        }
    }
}
