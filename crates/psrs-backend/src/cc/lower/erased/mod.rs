use super::super::layout::depends_on_type_variable;
use super::super::layout::{function_arrow_parameters, function_signature};
use super::super::{
    AggregateConvert, Assignment, AssignmentKind, Function, RecoveryEvidence, RefShape, Reference,
    SignatureId, UnaryOp, ValueConversion, ValueId, ValueShape,
};
use super::call::{
    is_function_type, is_generic_function_type, persist_reference, restore_reference,
};
use super::{FunctionLowerer, LambdaLowering};
use crate::BackendError;
use psrs_core::TypeId;

mod curried;

#[cfg(test)]
mod tests;

impl FunctionLowerer<'_> {
    /// Adapts the erased value of a generalized local binding to the concrete
    /// type at its use site. A `let`/`where` binding that is generalized is
    /// lowered at its polymorphic type, so its runtime value is erased. Using
    /// it at an instantiated type needs the same boxing of arguments and
    /// recovery of the result that the top-level polymorphic path performs.
    pub(super) fn adapt_erased_function_use(
        &mut self,
        value: ValueId,
        target_type: TypeId,
        span: psrs_span::TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(source_type) = self.erased_function_types.get(&value).copied() else {
            return Ok(value);
        };
        // Only an erased value needs the erased adaptation. The table also
        // records concrete function-typed results, which already carry their
        // exact closure shape.
        let erased = ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        });
        if self
            .values
            .iter()
            .find(|declaration| declaration.id == value)
            .map(|declaration| declaration.ty)
            != Some(erased)
        {
            return Ok(value);
        }
        if source_type == target_type || !is_function_type(self.module, target_type) {
            return Ok(value);
        }
        // Instantiating a type variable at a function type can flatten the use
        // into more parameters than the polymorphic value was lowered with
        // (`(id id) 42`). That higher-order case needs an intermediate closure
        // recovery this adapter does not build, so leave the value untouched
        // rather than report a misleading arity error.
        let source_arity = function_arrow_parameters(self.module, source_type).0.len();
        let target_arity = function_arrow_parameters(self.module, target_type).0.len();
        if source_arity != target_arity {
            return Ok(value);
        }
        self.adapt_erased_function_value(value, source_type, target_type, span, assignments)
    }

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
        let source_parameters = function_arrow_parameters(self.module, source_type).0;
        let target_parameters = function_arrow_parameters(self.module, target_type).0;
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
        if source_shape.parameters.len() != target_shape.parameters.len()
            || source_parameters.len() != target_parameters.len()
        {
            // A concrete curried function (the `ado` block's `\x -> \y -> ...`)
            // can be wider than the generic `a -> b` value it is adapted to:
            // the target's result is a type variable, so its own function
            // arguments are not part of the target arity. Partially apply the
            // source and return a closure that accepts them later.
            if source_shape.parameters.len() > target_shape.parameters.len()
                && source_parameters.len() > target_parameters.len()
                && is_erased_reference(target_shape.result)
            {
                return self.adapt_curried_function_value(
                    value,
                    source_type,
                    target_type,
                    source_signature_id,
                    target_signature_id,
                    &source_shape,
                    &target_shape,
                    span,
                    assignments,
                );
            }
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "generic function adapter has incompatible arity",
            )]);
        }

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
        let source_result_type = function_arrow_parameters(self.module, source_type).1;
        let target_result_type = function_arrow_parameters(self.module, target_type).1;
        // When the source result is a concrete function and the target result is
        // a generic function value, a plain erase cast would leave the caller
        // with a closure whose runtime call signature is concrete while the
        // generic use site calls it through the erased signature. Adapt the
        // nested function so the erased value is callable at the generic type.
        let result = if source_shape.result != target_shape.result
            && is_function_type(self.module, source_result_type)
            && is_generic_function_type(self.module, target_result_type)
        {
            adapter.adapt_erased_function_value(
                concrete_result,
                source_result_type,
                target_result_type,
                span,
                &mut adapter_assignments,
            )?
        } else {
            let conversion = adapter.typed_conversion(
                source_result_type,
                target_result_type,
                source_shape.result,
                target_shape.result,
                span,
            )?;
            adapter.emit_conversion(
                concrete_result,
                source_shape.result,
                target_shape.result,
                conversion,
                span,
                &mut adapter_assignments,
            )
        };
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
            // A `String` is a GC reference in the `eq` hierarchy, so it is
            // recovered by a cast to `(ref $string)`, not by the integer box.
            ValueShape::String => {
                let result = self.fresh(ValueShape::String);
                assignments.push(Assignment {
                    destination: result,
                    kind: AssignmentKind::AggregateConvert {
                        destination: result,
                        value,
                        conversion: AggregateConvert {
                            source: erased_reference_type(),
                            destination: ValueShape::String,
                            plan: ValueConversion::RecoverReference {
                                destination: ValueShape::String,
                                evidence: RecoveryEvidence::TypeInstantiation,
                            },
                        },
                    },
                    span,
                });
                Ok(result)
            }
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
                // The box stores Wasm i32; a Boolean destination is recovered
                // through an explicit `IntToBoolean` conversion so the product
                // projection keeps the box field's integer shape.
                let boxed_destination = if expected == ValueShape::Boolean {
                    self.fresh(ValueShape::Integer)
                } else {
                    self.fresh(expected)
                };
                assignments.push(Assignment {
                    destination: boxed_destination,
                    kind: AssignmentKind::ProductGet {
                        destination: boxed_destination,
                        representation: boxed_type,
                        field: 0,
                        value: concrete_box,
                    },
                    span,
                });
                if expected == ValueShape::Boolean {
                    let result = self.fresh(ValueShape::Boolean);
                    assignments.push(Assignment {
                        destination: result,
                        kind: AssignmentKind::Unary {
                            op: UnaryOp::IntToBoolean,
                            value: boxed_destination,
                        },
                        span,
                    });
                    Ok(result)
                } else {
                    Ok(boxed_destination)
                }
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

pub(super) fn erased_reference_type() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Erased,
    })
}

pub(super) fn closure_value_type() -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Aggregate,
    })
}

pub(super) fn closure_value_type_for(signature: SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

fn is_erased_reference(shape: ValueShape) -> bool {
    matches!(
        shape,
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Erased,
        })
    )
}
