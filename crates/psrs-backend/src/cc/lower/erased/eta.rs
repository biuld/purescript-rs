use super::super::super::layout::{depends_on_type_variable, function_arrow_parameters};
use super::super::super::{
    Assignment, AssignmentKind, Function, RefShape, Reference, Signature, SignatureId, ValueId,
};
use super::super::call::{persist_reference, restore_reference};
use super::super::{FunctionLowerer, LambdaLowering};
use super::{closure_value_type, closure_value_type_for, erased_reference_type};
use crate::BackendError;
use psrs_core::TypeId;

impl FunctionLowerer<'_> {
    /// Adapts a value whose runtime signature is narrower than the target by
    /// eta-expanding it. The target's flattened signature is wider because a
    /// type variable in the source result is instantiated at a function type
    /// (`(id id) 42`): the source consumes a prefix of the target arguments and
    /// its result is recovered as a function that the remaining arguments are
    /// applied to. This is the arity-general counterpart of the equal-arity
    /// adapter.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn adapt_eta_expanded_function_value(
        &mut self,
        value: ValueId,
        source_type: TypeId,
        target_type: TypeId,
        source_signature_id: SignatureId,
        target_signature_id: SignatureId,
        source_shape: &Signature,
        target_shape: &Signature,
        span: psrs_span::TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let source_arity = source_shape.parameters.len();
        let source_parameters = function_arrow_parameters(self.module, source_type).0;
        let target_parameters = function_arrow_parameters(self.module, target_type).0;

        let mut adapter = self.child_lowerer();
        let adapter_closure = adapter.fresh(closure_value_type());
        let mut adapter_parameters = vec![adapter_closure];
        let mut target_arguments = Vec::with_capacity(target_shape.parameters.len());
        for target_parameter in &target_shape.parameters {
            let parameter = adapter.fresh(*target_parameter);
            adapter_parameters.push(parameter);
            target_arguments.push(parameter);
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
        // Feed the source the prefix of the target arguments that its own
        // signature consumes, converting each from the target representation.
        let mut concrete_arguments = Vec::with_capacity(source_arity);
        for index in 0..source_arity {
            let conversion = adapter.typed_conversion(
                target_parameters[index],
                source_parameters[index],
                target_shape.parameters[index],
                source_shape.parameters[index],
                span,
            )?;
            let converted = adapter.emit_conversion(
                target_arguments[index],
                target_shape.parameters[index],
                source_shape.parameters[index],
                conversion,
                span,
                &mut adapter_assignments,
            );
            concrete_arguments.push(persist_reference(
                &mut adapter,
                converted,
                source_shape.parameters[index],
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
            .collect::<Vec<_>>();
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

        // The remaining target type is the source result's instantiation: a
        // function whose flattened parameters are the remaining target
        // arguments. Recover it and apply them.
        let remaining_type = peel_function_arrows(self.module, target_type, source_arity)
            .ok_or_else(|| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    span,
                    "eta-expanded adapter cannot peel the target type",
                )]
            })?;
        let Some(&remaining_signature_id) = self.function_types.get(&remaining_type) else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "eta-expanded adapter has no remaining function type",
            )]);
        };
        let source_result_type = function_arrow_parameters(self.module, source_type).1;
        let remaining_closure = closure_value_type_for(remaining_signature_id);
        let recover = adapter.typed_conversion(
            source_result_type,
            remaining_type,
            source_shape.result,
            remaining_closure,
            span,
        )?;
        let recovered = adapter.emit_conversion(
            concrete_result,
            source_shape.result,
            remaining_closure,
            recover,
            span,
            &mut adapter_assignments,
        );
        let remaining_result_type = function_arrow_parameters(self.module, remaining_type).1;
        let result_shape = adapter.value_shape(remaining_result_type, span)?;
        let result = adapter.fresh(result_shape);
        adapter_assignments.push(Assignment {
            destination: result,
            kind: AssignmentKind::IndirectCall {
                function: recovered,
                signature: remaining_signature_id,
                arguments: target_arguments[source_arity..].to_vec(),
            },
            span,
        });

        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let adapter_function = Function {
            symbol,
            name: format!("eta_adapter_{}", span.start),
            parameters: adapter_parameters,
            values: adapter.values,
            assignments: adapter_assignments,
            result,
            result_type: target_shape.result,
            span,
        };
        super::super::super::verify::verify_function(
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
            let erased = self.fresh(erased_reference_type());
            assignments.push(Assignment {
                destination: erased,
                kind: AssignmentKind::RepresentationCast {
                    destination: erased,
                    value: closure_result,
                    reference: Reference {
                        nullable: false,
                        heap: RefShape::Erased,
                    },
                },
                span,
            });
            Ok(erased)
        } else {
            Ok(closure_result)
        }
    }
}

/// Follows `count` ordinary function arrows, stopping at the callable
/// constructor boundary as [`function_arrow_parameters`] does. Returns the type
/// of the remaining value.
fn peel_function_arrows(
    module: &psrs_core::Module,
    mut id: TypeId,
    count: usize,
) -> Option<TypeId> {
    for _ in 0..count {
        let (_, result) = psrs_core::arrow_parts(&module.types, id)?;
        id = result;
    }
    Some(id)
}
