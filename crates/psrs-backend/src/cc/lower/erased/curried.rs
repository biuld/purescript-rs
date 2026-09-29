use super::super::super::layout::{
    depends_on_type_variable, function_arrow_parameters, function_signature,
};
use super::super::super::{
    Assignment, AssignmentKind, Function, RefShape, Reference, Signature, SignatureId, ValueId,
};
use super::super::{FunctionLowerer, LambdaLowering};
use super::{closure_value_type, closure_value_type_for, erased_reference_type};
use crate::BackendError;
use psrs_core::TypeId;

impl FunctionLowerer<'_> {
    /// Adapts a concrete curried function to a narrower generic function type
    /// by partially applying it. The outer closure accepts the target's
    /// arguments and returns an erased inner closure that accepts the source's
    /// remaining arguments. The inner closure captures the source and the
    /// prefix arguments, so it can still call the flattened source with every
    /// argument.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn adapt_curried_function_value(
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
        let prefix = target_shape.parameters.len();
        let remaining_type =
            peel_function_arrows(self.module, source_type, prefix).ok_or_else(|| {
                vec![BackendError::new(
                    "P8 closure conversion",
                    span,
                    "curried function adapter cannot peel the source type",
                )]
            })?;
        let Some(&remaining_signature_id) = self.function_types.get(&remaining_type) else {
            return Err(vec![BackendError::new(
                "P8 closure conversion",
                span,
                "curried function adapter has no remaining function type",
            )]);
        };
        let remaining_shape = function_signature(
            self.module,
            remaining_type,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            self.array_types,
            self.record_types,
            self.function_types,
        )?;
        let source_parameters = function_arrow_parameters(self.module, source_type).0;
        let target_parameters = function_arrow_parameters(self.module, target_type).0;

        let mut outer = self.child_lowerer();
        let outer_closure = outer.fresh(closure_value_type());
        let mut outer_parameters = vec![outer_closure];
        let mut target_arguments = Vec::with_capacity(prefix);
        for target_parameter in &target_shape.parameters {
            let parameter = outer.fresh(*target_parameter);
            outer_parameters.push(parameter);
            target_arguments.push(parameter);
        }
        let mut outer_assignments = Vec::new();
        let value_shape = self
            .values
            .iter()
            .find(|declaration| declaration.id == value)
            .map_or_else(erased_reference_type, |declaration| declaration.ty);
        let captured = outer.fresh(value_shape);
        outer_assignments.push(Assignment {
            destination: captured,
            kind: AssignmentKind::ClosureGetCapture {
                closure: outer_closure,
                index: 0,
            },
            span,
        });
        let source_closure = outer.fresh(closure_value_type_for(source_signature_id));
        outer_assignments.push(Assignment {
            destination: source_closure,
            kind: AssignmentKind::RepresentationCast {
                destination: source_closure,
                value: captured,
                reference: Reference {
                    nullable: false,
                    heap: RefShape::Closure(source_signature_id),
                },
            },
            span,
        });
        let mut inner_captures = vec![source_closure];
        for index in 0..prefix {
            let conversion = outer.typed_conversion(
                target_parameters[index],
                source_parameters[index],
                target_shape.parameters[index],
                source_shape.parameters[index],
                span,
            )?;
            let converted = outer.emit_conversion(
                target_arguments[index],
                target_shape.parameters[index],
                source_shape.parameters[index],
                conversion,
                span,
                &mut outer_assignments,
            );
            inner_captures.push(converted);
        }

        let mut inner = outer.child_lowerer();
        let inner_closure = inner.fresh(closure_value_type());
        let mut inner_parameters = vec![inner_closure];
        for parameter_type in &remaining_shape.parameters {
            inner_parameters.push(inner.fresh(*parameter_type));
        }
        let mut inner_assignments = Vec::new();
        let captured_source = inner.fresh(closure_value_type_for(source_signature_id));
        inner_assignments.push(Assignment {
            destination: captured_source,
            kind: AssignmentKind::ClosureGetCapture {
                closure: inner_closure,
                index: 0,
            },
            span,
        });
        let mut inner_arguments = Vec::with_capacity(source_parameters.len());
        for index in 0..prefix {
            let capture = inner.fresh(source_shape.parameters[index]);
            inner_assignments.push(Assignment {
                destination: capture,
                kind: AssignmentKind::ClosureGetCapture {
                    closure: inner_closure,
                    index: (index + 1) as u32,
                },
                span,
            });
            inner_arguments.push(capture);
        }
        inner_arguments.extend(inner_parameters[1..].iter().copied());
        let inner_result = inner.fresh(source_shape.result);
        inner_assignments.push(Assignment {
            destination: inner_result,
            kind: AssignmentKind::IndirectCall {
                function: captured_source,
                signature: source_signature_id,
                arguments: inner_arguments,
            },
            span,
        });
        let inner_symbol = outer.generated_symbols.borrow_mut().fresh(outer.owner);
        let inner_function = Function {
            symbol: inner_symbol,
            name: format!("curried_inner_{}", span.start),
            parameters: inner_parameters,
            values: inner.values,
            assignments: inner_assignments,
            result: inner_result,
            result_type: source_shape.result,
            span,
        };
        super::super::super::verify::verify_function(
            &inner_function,
            self.signatures,
            self.representations,
        )?;
        outer.generated.extend(inner.generated);
        outer.generated.push(inner_function);

        let inner_closure_value = outer.fresh(closure_value_type_for(remaining_signature_id));
        outer_assignments.push(Assignment {
            destination: inner_closure_value,
            kind: AssignmentKind::FunctionRef {
                function: inner_symbol,
                signature: remaining_signature_id,
                captures: inner_captures,
            },
            span,
        });
        let result = outer.fresh(target_shape.result);
        outer_assignments.push(Assignment {
            destination: result,
            kind: AssignmentKind::RepresentationCast {
                destination: result,
                value: inner_closure_value,
                reference: Reference {
                    nullable: false,
                    heap: RefShape::Erased,
                },
            },
            span,
        });
        let outer_symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let outer_function = Function {
            symbol: outer_symbol,
            name: format!("curried_outer_{}", span.start),
            parameters: outer_parameters,
            values: outer.values,
            assignments: outer_assignments,
            result,
            result_type: target_shape.result,
            span,
        };
        super::super::super::verify::verify_function(
            &outer_function,
            self.signatures,
            self.representations,
        )?;
        self.generated.extend(outer.generated);
        self.generated.push(outer_function);
        let closure_result = self.fresh(closure_value_type_for(target_signature_id));
        assignments.push(Assignment {
            destination: closure_result,
            kind: AssignmentKind::FunctionRef {
                function: outer_symbol,
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
/// of the remaining value, which may be a function returned by an effect.
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
