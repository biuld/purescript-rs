//! Constructor transport at checked generic boundaries.

use super::super::FunctionLowerer;
use super::{conversion_error, sequence};
use crate::{
    BackendError,
    cc::{RecoveryEvidence, RefShape, Reference, ValueConversion, ValueShape},
};
use psrs_core::{Instantiation, Type, TypeConstructor, TypeId};
use psrs_span::TextRange;

fn abstract_head(module: &psrs_core::Module, ty: TypeId) -> Option<psrs_hir::TypeVariableId> {
    let ty = super::super::super::layout::unquantified_type(module, ty);
    let Type::Application(head, _) = module.types.get(ty.0 as usize)? else {
        return None;
    };
    let Type::Variable(variable) = module.types.get(head.0 as usize)? else {
        return None;
    };
    Some(*variable)
}

fn callable(shape: ValueShape) -> Option<crate::cc::SignatureId> {
    match shape {
        ValueShape::Reference(Reference {
            heap: RefShape::Closure(id),
            ..
        }) => Some(id),
        _ => None,
    }
}

impl FunctionLowerer<'_> {
    pub(super) fn constructor_transport(
        &mut self,
        source: TypeId,
        target: TypeId,
        source_shape: ValueShape,
        target_shape: ValueShape,
        span: TextRange,
        evidence: Option<&Instantiation<'_>>,
    ) -> Result<Option<ValueConversion>, Vec<BackendError>> {
        let source_head = abstract_head(self.module, source);
        let target_head = abstract_head(self.module, target);
        let array_boundary = match (
            source_head,
            target_head,
            super::super::super::layout::array_element_type(self.module, source),
            super::super::super::layout::array_element_type(self.module, target),
        ) {
            (Some(variable), None, _, Some(element)) => Some((variable, false, target, element)),
            (None, Some(variable), Some(element), _) => Some((variable, true, source, element)),
            _ => None,
        };
        if let Some((variable, entering, concrete_type, element_type)) = array_boundary {
            let (constructor, fixed) = evidence
                .and_then(|proof| proof.constructor(variable))
                .ok_or_else(|| {
                    conversion_error(span, "array constructor transport has no checked binding")
                })?;
            if constructor != TypeConstructor::Array || !fixed.is_empty() {
                return Err(conversion_error(
                    span,
                    "array transport binding is not the unary Array constructor",
                ));
            }
            let erased_element = super::erased_shape();
            let protocol = self.representations.representations.iter().position(|representation|
                matches!(representation, crate::cc::Representation::Array { element } if *element == erased_element))
                .map(|index| crate::cc::ReprId(index as u32))
                .ok_or_else(|| conversion_error(span, "Array owner has no erased storage protocol"))?;
            let concrete = self
                .array_types
                .get(&concrete_type)
                .copied()
                .ok_or_else(|| conversion_error(span, "array transport has no concrete layout"))?;
            let element_shape = self.value_shape(element_type, span)?;
            let protocol_shape = ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(protocol),
            });
            if concrete == protocol {
                return Ok(Some(if entering {
                    ValueConversion::EraseReference
                } else {
                    ValueConversion::RecoverReference {
                        destination: target_shape,
                        evidence: RecoveryEvidence::TypeInstantiation,
                    }
                }));
            }
            return Ok(Some(if entering {
                let element = self.erase_payload(element_shape, span)?;
                sequence(vec![
                    ValueConversion::ArrayMap {
                        source: concrete,
                        target: protocol,
                        element: Box::new(element),
                    },
                    ValueConversion::EraseReference,
                ])
            } else {
                let element = self.recover_payload(element_shape, span)?;
                sequence(vec![
                    ValueConversion::RecoverReference {
                        destination: protocol_shape,
                        evidence: RecoveryEvidence::TypeInstantiation,
                    },
                    ValueConversion::ArrayMap {
                        source: protocol,
                        target: concrete,
                        element: Box::new(element),
                    },
                ])
            }));
        }
        let boundary = match (
            abstract_head(self.module, source),
            abstract_head(self.module, target),
        ) {
            (Some(variable), None) if callable(target_shape).is_some() => (variable, false),
            (None, Some(variable)) if callable(source_shape).is_some() => (variable, true),
            _ => return Ok(None),
        };
        let (variable, entering) = boundary;
        let (constructor, arguments) = evidence.and_then(|proof| proof.constructor(variable))
            .ok_or_else(|| vec![BackendError::new("P8 closure conversion", span,
                format!("abstract constructor transport has no checked constructor binding for {variable:?}, endpoints {source:?} -> {target:?}"))])?;
        let parameter_types = self
            .boundary
            .protocol_parameters(constructor, &arguments)
            .ok_or_else(|| {
                conversion_error(span, "constructor transport has no representation protocol")
            })?;
        let parameters = parameter_types
            .into_iter()
            .map(|ty| self.value_shape(ty, span))
            .collect::<Result<Vec<_>, _>>()?;
        let concrete = callable(if entering { source_shape } else { target_shape }).unwrap();
        let signature = self
            .representations
            .signature(concrete)
            .cloned()
            .ok_or_else(|| {
                conversion_error(span, "constructor transport has no concrete signature")
            })?;
        if signature.parameters != parameters {
            return Err(conversion_error(
                span,
                "constructor transport requires a callable segment adapter",
            ));
        }
        if let Some(result_type) = self.boundary.protocol_result(constructor)
            && signature.result != self.value_shape(result_type, span)?
        {
            return Err(conversion_error(
                span,
                "constructor transport result differs from its checked storage field",
            ));
        }
        let state = self.state_slot(&signature, span)?;
        let protocol = self.boundary.protocol_signature(concrete).ok_or_else(|| {
            conversion_error(span, "constructor transport has no registered protocol")
        })?;
        let payload = signature.result;
        let protocol_shape = ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Closure(protocol),
        });
        let identity_arguments = vec![ValueConversion::Identity; parameters.len()];
        let plan = if entering {
            let result = if let Some(state) = state {
                self.state_slot_result(state, true, span)?
            } else {
                self.erase_payload(payload, span)?
            };
            let adapter =
                self.physical_callable_plan(concrete, protocol, identity_arguments, result, span)?;
            sequence(vec![adapter, ValueConversion::EraseReference])
        } else {
            let result = if let Some(state) = state {
                self.state_slot_result(state, false, span)?
            } else {
                self.recover_payload(payload, span)?
            };
            let adapter =
                self.physical_callable_plan(protocol, concrete, identity_arguments, result, span)?;
            sequence(vec![
                ValueConversion::RecoverReference {
                    destination: protocol_shape,
                    evidence: RecoveryEvidence::TypeInstantiation,
                },
                adapter,
            ])
        };
        Ok(Some(plan))
    }
}
