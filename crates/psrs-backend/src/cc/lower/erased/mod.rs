use super::super::layout::{
    function_arrow_parameters, function_signature, function_type_signature,
};
use super::super::{
    Assignment, AssignmentKind, Function, RefShape, Reference, SignatureId, ValueId, ValueShape,
};
use super::call::{is_function_type, persist_reference, restore_reference};
use super::{FunctionLowerer, LambdaLowering};
use crate::BackendError;
use psrs_core::TypeId;

mod conversion;
mod curried;
mod eta;

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
        local: psrs_hir::LocalId,
        value: ValueId,
        target_type: TypeId,
        span: psrs_span::TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        // The binder's declared type, read from the lowering scope, is the
        // polymorphic source type of the lowered value. Deriving the adaptation
        // from this scope rather than a `ValueId` side table keeps it correct
        // across closure capture, lifting, and inlining, which all rewrite the
        // value.
        let Some(source_type) = self.local_types.get(&local).copied() else {
            return Ok(value);
        };
        // Equal normalized signatures need no adaptation, including generic
        // closures whose parameter and result representations already agree.
        if self.value_shape_of(value) == Some(self.value_shape(target_type, span)?) {
            return Ok(value);
        }
        if source_type == target_type || !is_function_type(self.module, target_type) {
            return Ok(value);
        }
        let evidence = self.boundary.instantiation_at(source_type, target_type);
        self.adapt_erased_function_value(
            value,
            source_type,
            target_type,
            span,
            assignments,
            evidence.as_ref(),
        )
    }

    fn value_shape_of(&self, value: ValueId) -> Option<ValueShape> {
        self.values
            .iter()
            .find(|declaration| declaration.id == value)
            .map(|declaration| declaration.ty)
    }

    pub(super) fn adapt_erased_function_value(
        &mut self,
        value: ValueId,
        source_type: TypeId,
        target_type: TypeId,
        span: psrs_span::TextRange,
        assignments: &mut Vec<Assignment>,
        instantiation: Option<&psrs_core::Instantiation<'_>>,
    ) -> Result<ValueId, Vec<BackendError>> {
        // A value that is not a function at its source type is an erased (or
        // concrete) value recovered at the target function type, such as a type
        // variable instantiated at a function type. No adapter closure is
        // needed; the representation conversion recovers the closure directly.
        if !is_function_type(self.module, source_type) {
            let source_shape = self.value_shape(source_type, span)?;
            let target_shape = self.value_shape(target_type, span)?;
            if source_shape == target_shape {
                return Ok(value);
            }
            let conversion = self.typed_conversion_with_instantiation(
                source_type,
                target_type,
                source_shape,
                target_shape,
                span,
                instantiation,
            )?;
            return Ok(self.emit_conversion(
                value,
                source_shape,
                target_shape,
                conversion,
                span,
                assignments,
            ));
        }
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
        let Some(source_signature_id) =
            function_type_signature(self.module, self.function_types, source_type)
        else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "concrete function adapter has no source function type",
            )]);
        };
        let Some(target_signature_id) =
            function_type_signature(self.module, self.function_types, target_type)
        else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "erased function adapter has no target function type",
            )]);
        };
        if source_shape.parameters.len() != target_shape.parameters.len()
            || source_parameters.len() != target_parameters.len()
        {
            // A type variable instantiated at a function type makes the target
            // wider than the source: the source's result is itself a function at
            // the instantiation, so its own arrows are flattened into extra
            // target parameters (`(id id) 42`). Eta-expand the adapter over the
            // full target arity and apply the remaining arguments to the
            // recovered result.
            if source_shape.parameters.len() < target_shape.parameters.len()
                && source_parameters.len() < target_parameters.len()
            {
                return self.adapt_eta_expanded_function_value(
                    value,
                    source_type,
                    target_type,
                    source_signature_id,
                    target_signature_id,
                    &source_shape,
                    &target_shape,
                    span,
                    assignments,
                    instantiation,
                );
            }
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
                    instantiation,
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
            let conversion = adapter.typed_conversion_with_instantiation(
                target_parameters[index],
                source_parameters[index],
                target_parameter,
                source_parameter,
                span,
                instantiation,
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
        let conversion = adapter.typed_conversion_with_instantiation(
            source_result_type,
            target_result_type,
            source_shape.result,
            target_shape.result,
            span,
            instantiation,
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
        Ok(closure_result)
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
