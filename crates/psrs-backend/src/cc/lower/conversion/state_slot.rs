//! Terminal callable protocol retaining a logical State and erased Step payload.
use super::*;
use crate::cc::state::StateCallProjection;
use crate::cc::{Signature, SignatureId};

#[derive(Clone, Copy)]
pub(super) struct StateSlot {
    pub signature: SignatureId,
    pub result: ValueShape,
    pub source_result: ValueShape,
    pub projection: StateCallProjection,
}

impl FunctionLowerer<'_> {
    pub(super) fn state_slot(
        &mut self,
        shape: &Signature,
        span: TextRange,
    ) -> Result<Option<StateSlot>, Vec<BackendError>> {
        let Some(projection) = StateCallProjection::checked(shape, self.representations)
            .map_err(|message| conversion_error(span, message))?
        else {
            return Ok(None);
        };
        let id = projection
            .payload_slot_signature(shape, self.representations)
            .map_err(|message| conversion_error(span, message))?;
        let result = self
            .representations
            .signature(id)
            .ok_or_else(|| conversion_error(span, "State payload slot is missing its signature"))?
            .result;
        Ok(Some(StateSlot {
            signature: id,
            result,
            source_result: shape.result,
            projection,
        }))
    }

    pub(super) fn state_slot_result(
        &mut self,
        slot: StateSlot,
        entering: bool,
        span: TextRange,
    ) -> Result<ValueConversion, Vec<BackendError>> {
        if slot.result == slot.source_result {
            return Ok(ValueConversion::Identity);
        }
        let ValueShape::Reference(Reference {
            heap: RefShape::Repr(concrete),
            ..
        }) = slot.source_result
        else {
            unreachable!()
        };
        let ValueShape::Reference(Reference {
            heap: RefShape::Repr(protocol),
            ..
        }) = slot.result
        else {
            unreachable!()
        };
        let payload = if entering {
            self.erase_payload(slot.projection.payload, span)?
        } else {
            self.recover_payload(slot.projection.payload, span)?
        };
        let mut fields = vec![ValueConversion::Identity; 2];
        fields[slot.projection.payload_field] = payload;
        Ok(ValueConversion::ProductMap {
            source: if entering { concrete } else { protocol },
            target: if entering { protocol } else { concrete },
            labels: self
                .representations
                .product_labels(concrete)
                .unwrap_or(&[])
                .to_vec(),
            fields,
        })
    }
}
