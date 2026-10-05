//! Constructor transport at checked generic boundaries.

use super::super::FunctionLowerer;
use super::{conversion_error, sequence};
use crate::{
    BackendError,
    cc::{RecoveryEvidence, RefShape, Reference, ValueConversion, ValueShape},
};
use psrs_core::{Instantiation, Type, TypeId};
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
        let signature = self.representations.signature(concrete).ok_or_else(|| {
            conversion_error(span, "constructor transport has no concrete signature")
        })?;
        if signature.parameters != parameters {
            return Err(conversion_error(
                span,
                "constructor transport requires a callable segment adapter",
            ));
        }
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
            let result = self.erase_payload(payload, span)?;
            let adapter =
                self.physical_callable_plan(concrete, protocol, identity_arguments, result, span)?;
            sequence(vec![adapter, ValueConversion::EraseReference])
        } else {
            let result = self.recover_payload(payload, span)?;
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
