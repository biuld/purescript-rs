use super::*;

impl FunctionLowerer<'_> {
    /// Under-application of a local closure or dictionary method. Mirrors
    /// [`Self::lower_partial_global_application`] but calls the captured callee
    /// value indirectly instead of a declaration symbol.
    pub(in crate::cc::lower::call) fn lower_indirect_partial_application(
        &mut self,
        application: IndirectPartialApplication<'_>,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let IndirectPartialApplication {
            expression,
            head,
            arguments,
            signature,
            signature_id,
            result_type,
        } = application;
        let Some(target_signature_id) =
            function_type_signature(self.module, self.function_types, expression.ty)
        else {
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
        let source_parameter_types = function_arrow_parameters(self.module, head.ty).0;
        if source_parameter_types.len() != signature.parameters.len() {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "partial application has an incomplete callee signature",
            )]);
        }

        // Convert each supplied argument to the callee's expected parameter
        // shape; each becomes a capture of the generated closure.
        let mut capture_conversions = Vec::with_capacity(arguments.len());
        for (index, argument) in arguments.iter().enumerate() {
            let expected = signature.parameters[index];
            let source_shape = self.value_shape(argument.ty, argument.span)?;
            let conversion = self.typed_conversion(
                argument.ty,
                source_parameter_types[index],
                source_shape,
                expected,
                expression.span,
            )?;
            capture_conversions.push((source_shape, conversion));
        }

        let callee = self.lower_value(head, assignments)?;
        let callee_type = self
            .values
            .iter()
            .find(|declaration| declaration.id == callee)
            .map(|declaration| declaration.ty)
            .ok_or_else(|| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    expression.span,
                    "partial application callee has no runtime type",
                )]
            })?;
        let mut captured = Vec::with_capacity(arguments.len() + 1);
        captured.push(callee);
        for (index, argument) in arguments.iter().enumerate() {
            let value = self.lower_value(argument, assignments)?;
            let expected = signature.parameters[index];
            let (source_shape, conversion) = capture_conversions[index].clone();
            let converted = self.emit_conversion(
                value,
                source_shape,
                expected,
                conversion,
                expression.span,
                assignments,
            );
            captured.push(converted);
        }

        let mut nested = self.child_lowerer();
        let closure_parameter = nested.fresh(closure_value_type());
        let mut parameters = vec![closure_parameter];
        let mut remaining_parameters = Vec::with_capacity(target_signature.parameters.len());
        for expected in &target_signature.parameters {
            let parameter = nested.fresh(*expected);
            parameters.push(parameter);
            remaining_parameters.push(parameter);
        }
        let mut nested_assignments = Vec::with_capacity(signature.parameters.len() + 1);
        let mut call_arguments = Vec::with_capacity(signature.parameters.len());
        let callee_capture = nested.fresh(callee_type);
        nested_assignments.push(Assignment {
            destination: callee_capture,
            kind: AssignmentKind::ClosureGetCapture {
                closure: closure_parameter,
                index: 0,
            },
            span: expression.span,
        });
        for (index, expected) in signature.parameters[..arguments.len()].iter().enumerate() {
            let destination = nested.fresh(*expected);
            nested_assignments.push(Assignment {
                destination,
                kind: AssignmentKind::ClosureGetCapture {
                    closure: closure_parameter,
                    index: (index + 1) as u32,
                },
                span: expression.span,
            });
            call_arguments.push(destination);
        }
        // A remaining parameter carries the expression's concrete shape, but
        // the callee stores its parameter erased. Convert it the way the
        // declaration partial application does.
        for (index, parameter) in remaining_parameters.into_iter().enumerate() {
            let position = arguments.len() + index;
            let source_type = source_parameter_types[position];
            let source_shape = target_signature.parameters[index];
            let expected = signature.parameters[position];
            let conversion = nested.typed_conversion(
                source_type,
                source_type,
                source_shape,
                expected,
                expression.span,
            )?;
            let converted = nested.emit_conversion(
                parameter,
                source_shape,
                expected,
                conversion,
                expression.span,
                &mut nested_assignments,
            );
            call_arguments.push(converted);
        }
        if signature.result != target_signature.result {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "partial application result does not match the callee result",
            )]);
        }
        let result = nested.fresh(signature.result);
        nested_assignments.push(Assignment {
            destination: result,
            kind: AssignmentKind::IndirectCall {
                function: callee_capture,
                signature: signature_id,
                arguments: call_arguments,
            },
            span: expression.span,
        });

        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let generated = Function {
            symbol,
            name: format!("partial_indirect_{}", expression.span.start),
            parameters,
            values: nested.values,
            assignments: nested_assignments,
            result,
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
        if result_type == closure_value_type_for(target_signature_id) {
            Ok(closure)
        } else if result_type
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
            Ok(erased)
        } else {
            Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "partial application result has the wrong runtime type",
            )])
        }
    }
}
