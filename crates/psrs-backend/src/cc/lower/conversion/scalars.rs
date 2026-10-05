use super::super::FunctionLowerer;
use super::{conversion_error, erased_shape, sequence};
use crate::{
    BackendError,
    cc::{BoxKind, RecoveryEvidence, RefShape, Reference, ReprId, ValueConversion, ValueShape},
};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(super) fn erase_payload(
        &mut self,
        shape: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        if shape == erased_shape() {
            return Ok(ValueConversion::Identity);
        }
        if let ValueShape::Reference(Reference {
            heap: RefShape::Closure(signature),
            ..
        }) = shape
        {
            let adapter = self.erase_function_slot(signature, span)?;
            return Ok(sequence(vec![adapter, ValueConversion::EraseReference]));
        }
        let boxed = match shape {
            ValueShape::Integer | ValueShape::Boolean => {
                self.box_plan(BoxKind::Integer, self.boxed_integer_type, span)?
            }
            ValueShape::Number => self.box_plan(BoxKind::Number, self.boxed_number_type, span)?,
            ValueShape::String | ValueShape::Reference(_) => {
                return Ok(ValueConversion::EraseReference);
            }
        };
        Ok(sequence(vec![boxed, ValueConversion::EraseReference]))
    }

    pub(super) fn recover_payload(
        &mut self,
        shape: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        if shape == erased_shape() {
            return Ok(ValueConversion::Identity);
        }
        if let ValueShape::Reference(Reference {
            heap: RefShape::Closure(signature),
            ..
        }) = shape
        {
            return self.recover_function_slot(signature, span);
        }
        match shape {
            ValueShape::Integer | ValueShape::Boolean => {
                self.unbox_plan(BoxKind::Integer, self.boxed_integer_type, shape, span)
            }
            ValueShape::Number => {
                self.unbox_plan(BoxKind::Number, self.boxed_number_type, shape, span)
            }
            ValueShape::String | ValueShape::Reference(_) => {
                Ok(ValueConversion::RecoverReference {
                    destination: shape,
                    evidence: RecoveryEvidence::TypeInstantiation,
                })
            }
        }
    }

    pub(super) fn box_plan(
        &self,
        kind: BoxKind,
        representation: Option<ReprId>,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        representation
            .map(|representation| ValueConversion::BoxScalar {
                kind,
                representation,
            })
            .ok_or_else(|| conversion_error(span, "erased scalar has no box representation"))
    }

    pub(super) fn unbox_plan(
        &self,
        kind: BoxKind,
        representation: Option<ReprId>,
        destination: ValueShape,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        representation
            .map(|representation| ValueConversion::UnboxScalar {
                kind,
                representation,
                destination,
            })
            .ok_or_else(|| conversion_error(span, "erased scalar has no box representation"))
    }
}
