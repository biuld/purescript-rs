use super::super::layout::depends_on_type_variable;
use super::super::layout::function_signature;
use super::super::{
    Assignment, AssignmentKind, Function, RefShape, Reference, SignatureId, ValueId, ValueShape,
};
use super::call::{persist_reference, restore_reference};
use super::{FunctionLowerer, LambdaLowering};
use crate::BackendError;
use psrs_core::{Type, TypeId};

impl FunctionLowerer<'_> {
    pub(super) fn adapt_erased_function_value(
        &mut self,
        value: ValueId,
        source_type: TypeId,
        target_type: TypeId,
        span: psrs_span::TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let source_shape = function_signature(
            self.module,
            source_type,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )?;
        let target_shape = function_signature(
            self.module,
            target_type,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )?;
        let source_parameters = function_parameter_types(self.module, source_type);
        let target_parameters = function_parameter_types(self.module, target_type);
        if source_shape.parameters.len() != target_shape.parameters.len()
            || source_parameters.len() != target_parameters.len()
        {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "generic function adapter has incompatible arity",
            )]);
        }
        let Some(&source_signature_id) = self.function_types.get(&source_type) else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "concrete function adapter has no source function type",
            )]);
        };
        let Some(&target_signature_id) = self.function_types.get(&target_type) else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "erased function adapter has no target function type",
            )]);
        };

        let mut adapter = self.child_lowerer();
        let adapter_closure = adapter.fresh(closure_value_type());
        let mut adapter_parameters = vec![adapter_closure];
        let mut adapter_arguments = Vec::with_capacity(target_parameters.len());
        for target_parameter in &target_shape.parameters {
            let parameter = adapter.fresh(*target_parameter);
            adapter_parameters.push(parameter);
            adapter_arguments.push(parameter);
        }
        let mut adapter_assignments = Vec::new();
        let captured_function = adapter.fresh(erased_reference_type());
        adapter_assignments.push(Assignment {
            destination: captured_function,
            kind: AssignmentKind::ClosureGetCapture {
                closure: adapter_closure,
                index: 0,
            },
            span,
        });
        let concrete_function = adapter.fresh(closure_value_type_for(source_signature_id));
        adapter_assignments.push(Assignment {
            destination: concrete_function,
            kind: AssignmentKind::RepresentationCast {
                destination: concrete_function,
                value: captured_function,
                reference: Reference {
                    nullable: false,
                    heap: RefShape::Closure(source_signature_id),
                },
            },
            span,
        });
        let concrete_function_shape = closure_value_type_for(source_signature_id);
        let (persistent_function, restore_function_shape) = persist_reference(
            &mut adapter,
            concrete_function,
            concrete_function_shape,
            span,
            &mut adapter_assignments,
        );
        let mut concrete_arguments = Vec::with_capacity(adapter_arguments.len());
        for (index, argument) in adapter_arguments.into_iter().enumerate() {
            let source_parameter = source_shape.parameters[index];
            let target_parameter = target_shape.parameters[index];
            let conversion = adapter.typed_conversion(
                target_parameters[index],
                source_parameters[index],
                target_parameter,
                source_parameter,
                span,
            )?;
            let converted = adapter.emit_conversion(
                argument,
                target_parameter,
                source_parameter,
                conversion,
                span,
                &mut adapter_assignments,
            );
            concrete_arguments.push(persist_reference(
                &mut adapter,
                converted,
                source_parameter,
                span,
                &mut adapter_assignments,
            ));
        }
        let concrete_function = match restore_function_shape {
            Some(shape) => restore_reference(
                &mut adapter,
                persistent_function,
                shape,
                span,
                &mut adapter_assignments,
            ),
            None => persistent_function,
        };
        let concrete_arguments = concrete_arguments
            .into_iter()
            .map(|(value, shape)| match shape {
                Some(shape) => {
                    restore_reference(&mut adapter, value, shape, span, &mut adapter_assignments)
                }
                None => value,
            })
            .collect();
        let concrete_result = adapter.fresh(source_shape.result);
        adapter_assignments.push(Assignment {
            destination: concrete_result,
            kind: AssignmentKind::IndirectCall {
                function: concrete_function,
                signature: source_signature_id,
                arguments: concrete_arguments,
            },
            span,
        });
        let source_result_type = function_result_type(self.module, source_type);
        let target_result_type = function_result_type(self.module, target_type);
        let conversion = adapter.typed_conversion(
            source_result_type,
            target_result_type,
            source_shape.result,
            target_shape.result,
            span,
        )?;
        let result = adapter.emit_conversion(
            concrete_result,
            source_shape.result,
            target_shape.result,
            conversion,
            span,
            &mut adapter_assignments,
        );
        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let adapter_function = Function {
            symbol,
            name: format!("erased_adapter_{}", span.start),
            parameters: adapter_parameters,
            values: adapter.values,
            assignments: adapter_assignments,
            result,
            result_type: target_shape.result,
            span,
        };
        super::super::verify::verify_function(
            &adapter_function,
            self.signatures,
            self.representations,
        )?;
        self.generated.extend(adapter.generated);
        self.generated.push(adapter_function);
        let closure_result = self.fresh(closure_value_type_for(target_signature_id));
        assignments.push(Assignment {
            destination: closure_result,
            kind: AssignmentKind::FunctionRef {
                function: symbol,
                signature: target_signature_id,
                captures: vec![value],
            },
            span,
        });
        if depends_on_type_variable(self.module, target_type) {
            let result = self.fresh(erased_reference_type());
            assignments.push(Assignment {
                destination: result,
                kind: AssignmentKind::RepresentationCast {
                    destination: result,
                    value: closure_result,
                    reference: Reference {
                        nullable: false,
                        heap: RefShape::Erased,
                    },
                },
                span,
            });
            Ok(result)
        } else {
            Ok(closure_result)
        }
    }

    pub(super) fn unbox_erased_value(
        &mut self,
        value: ValueId,
        expected: ValueShape,
        span: psrs_span::TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        match expected {
            ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            }) => Ok(value),
            ValueShape::Integer | ValueShape::Boolean => {
                let Some(boxed_type) = self.boxed_integer_type else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        span,
                        "polymorphic result has no integer box representation",
                    )]);
                };
                let concrete_box = self.fresh(ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(boxed_type),
                }));
                assignments.push(Assignment {
                    destination: concrete_box,
                    kind: AssignmentKind::RepresentationCast {
                        destination: concrete_box,
                        value,
                        reference: Reference {
                            nullable: false,
                            heap: RefShape::Repr(boxed_type),
                        },
                    },
                    span,
                });
                let result = self.fresh(expected);
                assignments.push(Assignment {
                    destination: result,
                    kind: AssignmentKind::ProductGet {
                        destination: result,
                        representation: boxed_type,
                        field: 0,
                        value: concrete_box,
                    },
                    span,
                });
                Ok(result)
            }
            ValueShape::Number => {
                let Some(boxed_type) = self.boxed_number_type else {
                    return Err(vec![BackendError::new(
                        "P8 closure conversion",
                        span,
                        "polymorphic result has no number box representation",
                    )]);
                };
                let concrete_box = self.fresh(ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(boxed_type),
                }));
                assignments.push(Assignment {
                    destination: concrete_box,
                    kind: AssignmentKind::RepresentationCast {
                        destination: concrete_box,
                        value,
                        reference: Reference {
                            nullable: false,
                            heap: RefShape::Repr(boxed_type),
                        },
                    },
                    span,
                });
                let result = self.fresh(ValueShape::Number);
                assignments.push(Assignment {
                    destination: result,
                    kind: AssignmentKind::ProductGet {
                        destination: result,
                        representation: boxed_type,
                        field: 0,
                        value: concrete_box,
                    },
                    span,
                });
                Ok(result)
            }
            ValueShape::Reference(reference) => {
                let result = self.fresh(ValueShape::Reference(reference));
                assignments.push(Assignment {
                    destination: result,
                    kind: AssignmentKind::RepresentationCast {
                        destination: result,
                        value,
                        reference,
                    },
                    span,
                });
                Ok(result)
            }
        }
    }
}

fn function_parameter_types(module: &psrs_core::Module, mut type_id: TypeId) -> Vec<TypeId> {
    let mut parameters = Vec::new();
    while let Some(Type::Function { parameter, result }) = module.types.get(type_id.0 as usize) {
        parameters.push(*parameter);
        type_id = *result;
    }
    parameters
}

fn function_result_type(module: &psrs_core::Module, mut type_id: TypeId) -> TypeId {
    while let Some(Type::Function { result, .. }) = module.types.get(type_id.0 as usize) {
        type_id = *result;
    }
    type_id
}

fn erased_reference_type() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

fn closure_value_type() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    })
}

fn closure_value_type_for(signature: SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}
