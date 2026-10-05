//! One callable protocol for functions stored under bare polymorphic values.
//! Erasure curries flattened calls into unary erased segments; recovery applies
//! those segments and adapts every argument and the final result explicitly.
use super::super::LambdaLowering;
use super::*;
use crate::cc::{Function, Signature, SignatureId};

fn closure(signature: SignatureId) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap: RefShape::Closure(signature),
    })
}

impl FunctionLowerer<'_> {
    fn slot_signature(&self, span: TextRange) -> Result<SignatureId, Vec<BackendError>> {
        self.boundary.function_slot_signature().ok_or_else(|| {
            conversion_error(span, "polymorphic function slot has no registered protocol")
        })
    }

    pub(super) fn erase_function_slot(
        &mut self,
        source: SignatureId,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        let slot = self.slot_signature(span)?;
        if source == slot {
            return Ok(ValueConversion::Identity);
        }
        let shape = self
            .representations
            .signature(source)
            .cloned()
            .ok_or_else(|| conversion_error(span, "function slot producer has no signature"))?;
        if shape.parameters.is_empty() {
            return Err(conversion_error(
                span,
                "function slot producer requires an argument",
            ));
        }
        let function = self.slot_segment(source, slot, &shape, 0, span)?;
        self.slot_factory(source, slot, function, span)
    }

    fn slot_segment(
        &mut self,
        source: SignatureId,
        slot: SignatureId,
        shape: &Signature,
        prefix: usize,
        span: TextRange,
    ) -> Result<psrs_hir::SymbolId, Vec<BackendError>> {
        let mut body = self.child_lowerer();
        let receiver = body.fresh(ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Aggregate,
        }));
        let input = body.fresh(erased_shape());
        let mut assignments = Vec::new();
        let producer = body.slot_capture(receiver, 0, closure(source), span, &mut assignments);
        let mut arguments = Vec::new();
        for index in 0..prefix {
            arguments.push(body.slot_capture(
                receiver,
                index as u32 + 1,
                shape.parameters[index],
                span,
                &mut assignments,
            ));
        }
        let conversion = body.recover_payload(shape.parameters[prefix], span)?;
        arguments.push(body.emit_conversion(
            input,
            erased_shape(),
            shape.parameters[prefix],
            conversion,
            span,
            &mut assignments,
        ));
        let result = if prefix + 1 == shape.parameters.len() {
            let result = body.fresh(shape.result);
            assignments.push(Assignment {
                destination: result,
                kind: AssignmentKind::IndirectCall {
                    function: producer,
                    signature: source,
                    arguments,
                },
                span,
            });
            let conversion = body.erase_payload(shape.result, span)?;
            body.emit_conversion(
                result,
                shape.result,
                erased_shape(),
                conversion,
                span,
                &mut assignments,
            )
        } else {
            let next = body.slot_segment(source, slot, shape, prefix + 1, span)?;
            let value = body.fresh(closure(slot));
            let mut captures = vec![producer];
            captures.extend(arguments);
            assignments.push(Assignment {
                destination: value,
                kind: AssignmentKind::FunctionRef {
                    function: next,
                    signature: slot,
                    captures,
                },
                span,
            });
            body.emit_conversion(
                value,
                closure(slot),
                erased_shape(),
                ValueConversion::EraseReference,
                span,
                &mut assignments,
            )
        };
        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let function = Function {
            symbol,
            name: format!("function_slot_segment_{prefix}_{}", span.start),
            parameters: vec![receiver, input],
            values: body.values,
            assignments,
            result,
            result_type: erased_shape(),
            span,
        };
        super::super::super::verify::verify_function(
            &function,
            self.signatures,
            self.representations,
        )?;
        self.generated.extend(body.generated);
        self.generated.push(function);
        Ok(symbol)
    }

    pub(super) fn recover_function_slot(
        &mut self,
        target: SignatureId,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        let slot = self.slot_signature(span)?;
        let cast = ValueConversion::RecoverReference {
            destination: closure(slot),
            evidence: RecoveryEvidence::TypeInstantiation,
        };
        if target == slot {
            return Ok(cast);
        }
        let shape = self
            .representations
            .signature(target)
            .cloned()
            .ok_or_else(|| conversion_error(span, "function slot consumer has no signature"))?;
        if shape.parameters.is_empty() {
            return Err(conversion_error(
                span,
                "function slot consumer requires an argument",
            ));
        }
        let mut body = self.child_lowerer();
        let receiver = body.fresh(ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Aggregate,
        }));
        let inputs: Vec<_> = shape
            .parameters
            .iter()
            .map(|shape| body.fresh(*shape))
            .collect();
        let mut parameters = vec![receiver];
        parameters.extend(inputs.iter().copied());
        let mut assignments = Vec::new();
        let mut current = body.slot_capture(receiver, 0, closure(slot), span, &mut assignments);
        let mut result = current;
        for (index, parameter_shape) in shape.parameters.iter().enumerate() {
            let input = inputs[index];
            let conversion = body.erase_payload(*parameter_shape, span)?;
            let argument = body.emit_conversion(
                input,
                *parameter_shape,
                erased_shape(),
                conversion,
                span,
                &mut assignments,
            );
            result = body.fresh(erased_shape());
            assignments.push(Assignment {
                destination: result,
                kind: AssignmentKind::IndirectCall {
                    function: current,
                    signature: slot,
                    arguments: vec![argument],
                },
                span,
            });
            if index + 1 < shape.parameters.len() {
                current = body.emit_conversion(
                    result,
                    erased_shape(),
                    closure(slot),
                    cast.clone(),
                    span,
                    &mut assignments,
                );
            }
        }
        let conversion = body.recover_payload(shape.result, span)?;
        let result = body.emit_conversion(
            result,
            erased_shape(),
            shape.result,
            conversion,
            span,
            &mut assignments,
        );
        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let function = Function {
            symbol,
            name: format!("function_slot_recovery_{}", span.start),
            parameters,
            values: body.values,
            assignments,
            result,
            result_type: shape.result,
            span,
        };
        super::super::super::verify::verify_function(
            &function,
            self.signatures,
            self.representations,
        )?;
        self.generated.extend(body.generated);
        self.generated.push(function);
        let factory = self.slot_factory(slot, target, symbol, span)?;
        Ok(sequence(vec![cast, factory]))
    }

    fn slot_capture(
        &mut self,
        receiver: ValueId,
        index: u32,
        shape: ValueShape,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> ValueId {
        let value = self.fresh(shape);
        assignments.push(Assignment {
            destination: value,
            kind: AssignmentKind::ClosureGetCapture {
                closure: receiver,
                index,
            },
            span,
        });
        value
    }

    fn slot_factory(
        &mut self,
        source: SignatureId,
        target: SignatureId,
        body: psrs_hir::SymbolId,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        let mut factory = self.child_lowerer();
        let input = factory.fresh(closure(source));
        let output = factory.fresh(closure(target));
        let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
        let function = Function {
            symbol,
            name: format!("function_slot_factory_{}", span.start),
            parameters: vec![input],
            values: factory.values,
            assignments: vec![Assignment {
                destination: output,
                kind: AssignmentKind::FunctionRef {
                    function: body,
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
