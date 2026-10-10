//! One callable protocol for functions stored under bare polymorphic values.
//! Erasure curries flattened calls into unary erased segments; recovery applies
//! those segments and adapts every argument and the final result explicitly.
use super::super::LambdaLowering;
use super::state_slot::StateSlot;
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
        let state = self.state_slot(&shape, span)?;
        let initial = if shape.parameters[0] == ValueShape::State {
            state.unwrap().signature
        } else {
            slot
        };
        let function = self.slot_segment(source, slot, &shape, 0, state, span)?;
        self.slot_factory(source, initial, function, span)
    }

    fn slot_segment(
        &mut self,
        source: SignatureId,
        slot: SignatureId,
        shape: &Signature,
        prefix: usize,
        state: Option<StateSlot>,
        span: TextRange,
    ) -> Result<psrs_hir::SymbolId, Vec<BackendError>> {
        let mut body = self.child_lowerer();
        let receiver = body.fresh(ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Aggregate,
        }));
        let terminal = shape.parameters[prefix] == ValueShape::State;
        let input_shape = if terminal {
            ValueShape::State
        } else {
            erased_shape()
        };
        let result_shape = if terminal {
            state.unwrap().result
        } else {
            erased_shape()
        };
        let input = body.fresh(input_shape);
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
        let conversion = if terminal {
            ValueConversion::Identity
        } else {
            body.recover_payload(shape.parameters[prefix], span)?
        };
        arguments.push(body.emit_conversion(
            input,
            input_shape,
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
            let conversion = if terminal {
                body.state_slot_result(state.unwrap(), true, span)?
            } else {
                body.erase_payload(shape.result, span)?
            };
            body.emit_conversion(
                result,
                shape.result,
                result_shape,
                conversion,
                span,
                &mut assignments,
            )
        } else {
            let next = body.slot_segment(source, slot, shape, prefix + 1, state, span)?;
            let next_slot = if shape.parameters[prefix + 1] == ValueShape::State {
                state.unwrap().signature
            } else {
                slot
            };
            let value = body.fresh(closure(next_slot));
            let mut captures = vec![producer];
            captures.extend(arguments);
            assignments.push(Assignment {
                destination: value,
                kind: AssignmentKind::FunctionRef {
                    function: next,
                    signature: next_slot,
                    captures,
                },
                span,
            });
            body.emit_conversion(
                value,
                closure(next_slot),
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
            result_type: result_shape,
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
        let state = self.state_slot(&shape, span)?;
        let initial = if shape.parameters[0] == ValueShape::State {
            state.unwrap().signature
        } else {
            slot
        };
        let cast = ValueConversion::RecoverReference {
            destination: closure(initial),
            evidence: RecoveryEvidence::TypeInstantiation,
        };
        if target == initial {
            return Ok(cast);
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
        let mut current = body.slot_capture(receiver, 0, closure(initial), span, &mut assignments);
        let mut result = current;
        for (index, parameter_shape) in shape.parameters.iter().enumerate() {
            let input = inputs[index];
            let terminal = *parameter_shape == ValueShape::State;
            let argument_shape = if terminal {
                ValueShape::State
            } else {
                erased_shape()
            };
            let invoked = if terminal {
                state.unwrap().signature
            } else {
                slot
            };
            let conversion = if terminal {
                ValueConversion::Identity
            } else {
                body.erase_payload(*parameter_shape, span)?
            };
            let argument = body.emit_conversion(
                input,
                *parameter_shape,
                argument_shape,
                conversion,
                span,
                &mut assignments,
            );
            result = body.fresh(if terminal {
                state.unwrap().result
            } else {
                erased_shape()
            });
            assignments.push(Assignment {
                destination: result,
                kind: AssignmentKind::IndirectCall {
                    function: current,
                    signature: invoked,
                    arguments: vec![argument],
                },
                span,
            });
            if index + 1 < shape.parameters.len() {
                let next_slot = if shape.parameters[index + 1] == ValueShape::State {
                    state.unwrap().signature
                } else {
                    slot
                };
                current = body.emit_conversion(
                    result,
                    erased_shape(),
                    closure(next_slot),
                    ValueConversion::RecoverReference {
                        destination: closure(next_slot),
                        evidence: RecoveryEvidence::TypeInstantiation,
                    },
                    span,
                    &mut assignments,
                );
            }
        }
        let source_result = state.map_or(erased_shape(), |state| state.result);
        let conversion = if let Some(state) = state {
            body.state_slot_result(state, false, span)?
        } else {
            body.recover_payload(shape.result, span)?
        };
        let result = body.emit_conversion(
            result,
            source_result,
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
        let factory = self.slot_factory(initial, target, symbol, span)?;
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
