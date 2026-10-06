use super::*;

impl FunctionLowerer<'_> {
    pub(in crate::cc::lower::call) fn lower_partial_global_application(
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

        let declared_parameters = callable_parameter_types(self.module, function, callable_type);
        if declared_parameters.len() != source_signature.parameters.len() {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                expression.span,
                "partial application has an incomplete declaration signature",
            )]);
        }
        let declaration = self
            .module
            .declarations
            .iter()
            .find(|item| item.symbol == function);
        let evidence = declaration.and_then(|declaration| {
            self.boundary.checked_instantiation(
                declaration.ty,
                &declaration.quantified,
                callable_type,
            )
        });
        let (use_parameters, _) = function_arrow_parameters(self.module, callable_type);
        let remaining_count = source_signature.parameters.len() - arguments.len();
        if target_signature.parameters.len() < remaining_count {
            return Err(vec![BackendError::invalid_ir(
                "P8 closure conversion",
                expression.span,
                "partial application target omits a declaration parameter",
            )]);
        }
        let capture_conversions = arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                let source_type = declared_parameters[index];
                let expected = source_signature.parameters[index];
                let source_shape = self.value_shape(argument.ty, argument.span)?;
                let conversion = self.typed_conversion_with_instantiation(
                    argument.ty,
                    source_type,
                    source_shape,
                    expected,
                    expression.span,
                    evidence.as_ref(),
                )?;
                Ok((source_shape, conversion))
            })
            .collect::<Result<Vec<_>, Vec<BackendError>>>()?;
        let mut captured = Vec::with_capacity(arguments.len());
        for (index, argument) in arguments.iter().enumerate() {
            let value = self.lower_value(argument, assignments)?;
            let expected = source_signature.parameters[index];
            let (source_shape, conversion) = capture_conversions[index].clone();
            let converted = self.emit_conversion(
                value,
                source_shape,
                expected,
                conversion,
                expression.span,
                assignments,
            );
            let later_reconstructs = capture_conversions[index + 1..]
                .iter()
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
        let mut remaining_shapes = Vec::with_capacity(target_signature.parameters.len());
        for expected in target_signature.parameters {
            let parameter = nested.fresh(expected);
            parameters.push(parameter);
            remaining_parameters.push(parameter);
            remaining_shapes.push(expected);
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
        // A remaining parameter carries the callable type's concrete shape here,
        // but the callee's declaration may store a polymorphic parameter erased.
        // Convert it the way a saturated call does; an identity conversion when
        // the shapes already agree keeps the monomorphic case unchanged.
        for (index, parameter) in remaining_parameters
            .iter()
            .copied()
            .take(remaining_count)
            .enumerate()
        {
            let position = captured.len() + index;
            let source_type = declared_parameters[position];
            let source_shape = remaining_shapes[index];
            let expected = source_signature.parameters[position];
            let use_type = use_parameters.get(position).copied().ok_or_else(|| {
                vec![BackendError::invalid_ir(
                    "P8 closure conversion",
                    expression.span,
                    "partial application parameter has no checked use type",
                )]
            })?;
            let conversion = nested.typed_conversion_with_instantiation(
                use_type,
                source_type,
                source_shape,
                expected,
                expression.span,
                evidence.as_ref(),
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
        let result = nested.fresh(source_signature.result);
        nested_assignments.push(Assignment {
            destination: result,
            kind: AssignmentKind::DirectCall {
                function,
                arguments: call_arguments,
            },
            span: expression.span,
        });
        let (result, source_result_shape, source_result_type) = if remaining_parameters.len()
            > remaining_count
        {
            let source_type = callable_result_type(self.module, function, callable_type)
                .ok_or_else(|| {
                    vec![BackendError::invalid_ir(
                        "P8 closure conversion",
                        expression.span,
                        "partial application result has no declaration type",
                    )]
                })?;
            // Peel only the declaration's checked ordinary call prefix. Any
            // remaining arrows belong to the value returned by that call.
            let mut cursor = callable_type;
            for _ in 0..source_signature.parameters.len() {
                while let Some((_, body)) = psrs_core::forall_parts(&self.module.types, cursor) {
                    cursor = body;
                }
                let (_, tail) =
                    psrs_core::arrow_parts(&self.module.types, cursor).ok_or_else(|| {
                        vec![BackendError::invalid_ir(
                            "P8 closure conversion",
                            expression.span,
                            "partial application calling prefix has no checked use arrow",
                        )]
                    })?;
                cursor = tail;
            }
            let returned_signature =
                function_type_signature(self.module, self.function_types, cursor).ok_or_else(
                    || {
                        vec![BackendError::invalid_ir(
                            "P8 closure conversion",
                            expression.span,
                            "partial application returned callable has no signature",
                        )]
                    },
                )?;
            let returned = self
                .representations
                .signature(returned_signature)
                .cloned()
                .ok_or_else(|| {
                    vec![BackendError::invalid_ir(
                        "P8 closure conversion",
                        expression.span,
                        "partial application returned callable signature is absent",
                    )]
                })?;
            let tail_parameters = &remaining_parameters[remaining_count..];
            if returned.parameters != remaining_shapes[remaining_count..] {
                return Err(vec![BackendError::invalid_ir(
                    "P8 closure conversion",
                    expression.span,
                    "partial application returned callable parameters disagree with its target",
                )]);
            }
            let returned_shape = closure_value_type_for(returned_signature);
            let conversion = nested.typed_conversion_with_instantiation(
                source_type,
                cursor,
                source_signature.result,
                returned_shape,
                expression.span,
                evidence.as_ref(),
            )?;
            let callable = nested.emit_conversion(
                result,
                source_signature.result,
                returned_shape,
                conversion,
                expression.span,
                &mut nested_assignments,
            );
            let destination = nested.fresh(returned.result);
            nested_assignments.push(Assignment {
                destination,
                kind: AssignmentKind::IndirectCall {
                    function: callable,
                    signature: returned_signature,
                    arguments: tail_parameters.to_vec(),
                },
                span: expression.span,
            });
            let use_result = function_result_type(self.module, cursor);
            (destination, returned.result, Some(use_result))
        } else {
            (result, source_signature.result, None)
        };
        // The callee's declaration result is the un-instantiated shape, but the
        // closure signature is the partially applied expression's result. A
        // polymorphic declaration such as `pure :: a -> Effect a` returns an
        // erased value while the expression's result is concrete, so convert
        // before the generated function returns.
        let result_conversion = if source_result_shape == target_signature.result {
            ValueConversion::Identity
        } else {
            let source_result_type = source_result_type
                .or_else(|| callable_result_type(self.module, function, callable_type))
                .ok_or_else(|| {
                    vec![BackendError::new(
                        "P8 closure conversion",
                        expression.span,
                        "partial application callee has no declaration result type",
                    )]
                })?;
            let destination_result_type = function_result_type(self.module, expression.ty);
            nested.typed_conversion_with_instantiation(
                source_result_type,
                destination_result_type,
                source_result_shape,
                target_signature.result,
                expression.span,
                evidence.as_ref(),
            )?
        };
        let converted_result = nested.emit_conversion(
            result,
            source_result_shape,
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
