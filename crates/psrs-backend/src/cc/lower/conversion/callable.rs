//! Emission of callable plans with representation-only endpoints.

use super::super::{FunctionLowerer, LambdaLowering};
use crate::BackendError;
use crate::cc::{
    Assignment, AssignmentKind, Function, RefShape, Reference, SignatureId, ValueConversion,
    ValueShape,
};
use psrs_span::TextRange;

fn closure(signature: SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

impl FunctionLowerer<'_> {
    /// Emits a completed callable plan. No source matching occurs here: the
    /// two signatures and recursive argument/result plans are already fixed.
    pub(super) fn physical_callable_plan(
        &mut self,
        source: SignatureId,
        target: SignatureId,
        arguments: Vec<ValueConversion>,
        result_plan: ValueConversion,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        if source == target
            && arguments
                .iter()
                .all(|plan| matches!(plan, ValueConversion::Identity))
            && matches!(result_plan, ValueConversion::Identity)
        {
            return Ok(ValueConversion::Identity);
        }
        let source_shape = self
            .representations
            .signature(source)
            .cloned()
            .ok_or_else(|| {
                super::conversion_error(span, "callable plan has no producer signature")
            })?;
        let target_shape = self
            .representations
            .signature(target)
            .cloned()
            .ok_or_else(|| {
                super::conversion_error(span, "callable plan has no consumer signature")
            })?;
        if source_shape.parameters.len() != target_shape.parameters.len()
            || arguments.len() != target_shape.parameters.len()
        {
            return Err(super::conversion_error(
                span,
                "callable plan has incompatible parameter counts",
            ));
        }
        let mut body = self.child_lowerer();
        let receiver = body.fresh(ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Aggregate,
        }));
        let mut parameters = vec![receiver];
        let inputs = target_shape
            .parameters
            .iter()
            .map(|shape| body.fresh(*shape))
            .collect::<Vec<_>>();
        parameters.extend(inputs.iter().copied());
        let mut values = Vec::new();
        let mut assignments = Vec::new();
        let captured = body.fresh(super::erased_shape());
        assignments.push(Assignment {
            destination: captured,
            kind: AssignmentKind::ClosureGetCapture {
                closure: receiver,
                index: 0,
            },
            span,
        });
        let producer = body.fresh(closure(source));
        assignments.push(Assignment {
            destination: producer,
            kind: AssignmentKind::RepresentationCast {
                destination: producer,
                value: captured,
                reference: Reference {
                    nullable: false,
                    heap: RefShape::Closure(source),
                },
            },
            span,
        });
        // Preserve references across recursive plans that can allocate loops.
        let (producer, producer_shape) = super::super::call::persist_reference(
            &mut body,
            producer,
            closure(source),
            span,
            &mut assignments,
        );
        for (index, plan) in arguments.into_iter().enumerate() {
            let parameter = inputs[index];
            let value = body.emit_conversion(
                parameter,
                target_shape.parameters[index],
                source_shape.parameters[index],
                plan,
                span,
                &mut assignments,
            );
            values.push(super::super::call::persist_reference(
                &mut body,
                value,
                source_shape.parameters[index],
                span,
                &mut assignments,
            ));
        }
        let producer = match producer_shape {
            Some(shape) => super::super::call::restore_reference(
                &mut body,
                producer,
                shape,
                span,
                &mut assignments,
            ),
            None => producer,
        };
        let values = values
            .into_iter()
            .map(|(value, shape)| match shape {
                Some(shape) => super::super::call::restore_reference(
                    &mut body,
                    value,
                    shape,
                    span,
                    &mut assignments,
                ),
                None => value,
            })
            .collect();
        let value = body.fresh(source_shape.result);
        assignments.push(Assignment {
            destination: value,
            kind: AssignmentKind::IndirectCall {
                function: producer,
                signature: source,
                arguments: values,
            },
            span,
        });
        let result = body.emit_conversion(
            value,
            source_shape.result,
            target_shape.result,
            result_plan,
            span,
            &mut assignments,
        );
        let body_symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let function = Function {
            symbol: body_symbol,
            name: format!("protocol_adapter_{}", span.start),
            parameters,
            values: body.values,
            assignments,
            result,
            result_type: target_shape.result,
            span,
        };
        super::super::super::verify::verify_function(
            &function,
            self.signatures,
            self.representations,
        )?;
        self.generated.extend(body.generated);
        self.generated.push(function);

        let mut factory = self.child_lowerer();
        let input = factory.fresh(closure(source));
        let output = factory.fresh(closure(target));
        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let function = Function {
            symbol,
            name: format!("protocol_adapter_factory_{}", span.start),
            parameters: vec![input],
            values: factory.values,
            assignments: vec![Assignment {
                destination: output,
                kind: AssignmentKind::FunctionRef {
                    function: body_symbol,
                    signature: target,
                    captures: vec![input],
                },
                span,
            }],
            result: output,
            result_type: closure(target),
            span,
        };
        super::super::super::verify::verify_function(
            &function,
            self.signatures,
            self.representations,
        )?;
        self.generated.push(function);
        Ok(ValueConversion::FunctionAdapter {
            function: symbol,
            source: closure(source),
            destination: closure(target),
        })
    }
}
